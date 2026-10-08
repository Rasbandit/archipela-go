//! Standalone ("solo") mode: the app's own mini-randomizer. Mirrors the apworld (same slots, item pool, IDs)
//! so every feature works without an Archipelago server. See docs/superpowers/specs/2026-10-08-v1-quests-realms-design.md.

use std::collections::BTreeMap;

use rand::rngs::StdRng;
use rand::seq::{IndexedRandom, SliceRandom};
use rand::{Rng, SeedableRng};
use serde::{Deserialize, Serialize};

use crate::catalog::Mode;
use crate::slot::{check_goal_specs, GoalMode, GoalSpec, QuestSlot, SlotData, ZoneSlot, CURRENT_SCHEMA};

pub const ID_OFFSET: i64 = 8_902_400_000_000;
pub const BLOCK_SIZE: i64 = 1000;
pub const GOALS: [&str; 12] = [
    "macguffin_short",
    "macguffin_long",
    "all_trips",
    "boss",
    "treasure_hunt",
    "zone_conqueror",
    "well_rounded",
    "quest_dex",
    "marathon",
    "explorer",
    "streak",
    "boss_rush",
];
pub const FAMILIES: [&str; 10] = ["reach", "dwell", "landmark", "trail", "park", "water", "courier", "explore", "steps", "away"];
const DIFFICULTIES: [&str; 3] = ["easy", "medium", "hard"];
const BANDS: [(u8, u8); 3] = [(1, 3), (4, 7), (8, 10)];
const REACH_WEIGHT: usize = 3;
pub const HONOR_TRAPS: [&str; 5] = ["Push Up Trap", "Socializing Trap", "Sit Up Trap", "Jumping Jack Trap", "Touch Grass Trap"];
pub const FILLERS: [&str; 2] = ["Hydrate!", "Take a Breather!"];

#[must_use]
pub fn trap_item(key: &str) -> Option<&'static str> {
    Some(match key {
        "freeze" => "Freeze Trap",
        "fog" => "Fog Of War Trap",
        "shuffle" => "Shuffle Trap",
        "silence" => "Silence Trap",
        "leash" => "Leash Trap",
        "detour" => "Detour Trap",
        "toll" => "Toll Trap",
        "slow" => "Slow Trap",
        _ => return None,
    })
}

#[must_use]
pub fn tool_for(mode: Mode) -> Option<&'static str> {
    match mode {
        Mode::Walk => None,
        Mode::Run => Some("Running Shoes"),
        Mode::Bike => Some("Bike"),
        Mode::Drive => Some("Car"),
    }
}

fn family_allows(family: &str, mode: Mode) -> bool {
    match family {
        "reach" | "dwell" | "landmark" | "courier" | "away" => true,
        "explore" | "trail" | "water" => mode != Mode::Drive,
        "park" | "steps" => matches!(mode, Mode::Walk | Mode::Run),
        _ => false,
    }
}

fn mode_index(m: Mode) -> i64 {
    Mode::ALL.iter().position(|x| *x == m).unwrap_or(0) as i64
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SoloOptions {
    /// The win condition. When `goals` is not empty it takes over (and these two are only the single-goal form).
    pub goal: String,
    pub goal_target: u32,
    pub goals: Vec<GoalSpec>,
    pub goal_mode: GoalMode,
    pub goal_need: u32,
    pub number_of_trips: u32,
    pub zone_modes: Vec<Mode>,
    pub easy_share: u32,
    pub medium_share: u32,
    pub hard_share: u32,
    pub minutes_per_tier: u32,
    pub min_distance_m: u32,
    pub quest_types: Vec<String>,
    /// Quest types for each zone, in zone order. A missing or empty entry uses `quest_types`.
    pub zone_quest_types: Vec<Vec<String>>,
    pub enabled_traps: Vec<String>,
    pub trap_rate: u32,
    pub enable_effort_reductions: bool,
    pub enable_scouting: bool,
    pub enable_collection: bool,
    pub reduction_percent: u32,
    pub fog_of_war: bool,
    pub return_home: bool,
}

impl Default for SoloOptions {
    fn default() -> Self {
        Self {
            goal: "macguffin_short".into(),
            goal_target: 0,
            goals: vec![],
            goal_mode: GoalMode::All,
            goal_need: 0,
            number_of_trips: 100,
            zone_modes: vec![Mode::Walk],
            easy_share: 50,
            medium_share: 35,
            hard_share: 15,
            minutes_per_tier: 10,
            min_distance_m: 150,
            quest_types: FAMILIES.iter().map(std::string::ToString::to_string).collect(),
            zone_quest_types: vec![],
            enabled_traps: ["freeze", "fog", "shuffle", "silence", "leash", "detour", "toll", "slow", "honor"]
                .iter()
                .map(std::string::ToString::to_string)
                .collect(),
            trap_rate: 30,
            enable_effort_reductions: false,
            enable_scouting: false,
            enable_collection: false,
            reduction_percent: 8,
            fog_of_war: false,
            return_home: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SoloGame {
    pub slot: SlotData,
    /// `location_id` -> item name found there.
    pub rewards: BTreeMap<i64, String>,
}

fn letters_for(goal: &str) -> &'static str {
    match goal {
        "macguffin_long" => "ARCHIPELAGO",
        "macguffin_short" | "treasure_hunt" => "APGO",
        _ => "",
    }
}

fn has_boss(goal: &str) -> bool {
    matches!(goal, "boss" | "treasure_hunt")
}

/// The letters a set of goals needs: the long word covers the short one, so the longest asked for is enough.
fn letters_for_goals(goals: &[GoalSpec]) -> &'static str {
    goals.iter().map(|g| letters_for(&g.id)).max_by_key(|l| l.len()).unwrap_or("")
}

fn has_boss_goal(goals: &[GoalSpec]) -> bool {
    goals.iter().any(|g| has_boss(&g.id))
}

impl SoloOptions {
    /// The win conditions: the `goals` list, or the single `goal`.
    #[must_use]
    pub fn goal_list(&self) -> Vec<GoalSpec> {
        if self.goals.is_empty() {
            vec![GoalSpec { id: self.goal.clone(), target: self.goal_target }]
        } else {
            self.goals.clone()
        }
    }
}

/// Largest-remainder split of `total` by weights (zero weights get zero).
fn split(total: u32, weights: &[u32]) -> Vec<u32> {
    let sum: u32 = weights.iter().sum();
    if sum == 0 {
        return vec![0; weights.len()];
    }
    let exact: Vec<f64> = weights.iter().map(|w| f64::from(total) * f64::from(*w) / f64::from(sum)).collect();
    let mut out: Vec<u32> = exact.iter().map(|e| e.floor() as u32).collect();
    let mut rem: Vec<usize> = (0..weights.len()).collect();
    rem.sort_by(|&a, &b| (exact[b] - exact[b].floor()).total_cmp(&(exact[a] - exact[a].floor())));
    for i in rem.into_iter().take((total - out.iter().sum::<u32>()) as usize) {
        out[i] += 1;
    }
    out
}

pub fn validate(o: &SoloOptions) -> Result<(), String> {
    if o.zone_modes.is_empty() || o.zone_modes.len() > 6 {
        return Err("zone_modes needs 1 to 6 zones".into());
    }
    check_goal_specs(&o.goal_list(), o.goal_mode, o.goal_need)?;
    if o.easy_share + o.medium_share + o.hard_share == 0 {
        return Err("at least one difficulty share must be above 0".into());
    }
    if !(1..=1000).contains(&o.number_of_trips) {
        return Err("number_of_trips must be 1..1000".into());
    }
    let zones = o.zone_modes.len() as u32;
    let tools = tool_names(o).len() as u32;
    let mandatory = letters_for_goals(&o.goal_list()).len() as u32 + (zones - 1) + tools;
    if o.number_of_trips < mandatory.max(zones) {
        return Err(format!("number_of_trips ({}) is too small; this setup needs at least {}", o.number_of_trips, mandatory.max(zones)));
    }
    if let Some(bad) = o.quest_types.iter().chain(o.zone_quest_types.iter().flatten()).find(|q| !FAMILIES.contains(&q.as_str())) {
        return Err(format!("unknown quest type {bad}"));
    }
    Ok(())
}

/// Tools needed by zones >= 2 whose mode differs from zone 1's (walk never needs one).
#[must_use]
pub fn tool_names(o: &SoloOptions) -> Vec<&'static str> {
    let first = o.zone_modes[0];
    let mut v: Vec<&'static str> = Vec::new();
    for m in o.zone_modes.iter().skip(1) {
        if *m != first {
            if let Some(t) = tool_for(*m) {
                if !v.contains(&t) {
                    v.push(t);
                }
            }
        }
    }
    v
}

pub fn generate(o: &SoloOptions, seed: u64) -> Result<SoloGame, String> {
    validate(o)?;
    let mut rng = StdRng::seed_from_u64(seed);
    let zn = o.zone_modes.len();
    let first = o.zone_modes[0];

    let zones: Vec<ZoneSlot> = o
        .zone_modes
        .iter()
        .enumerate()
        .map(|(i, m)| ZoneSlot {
            id: i as u32 + 1,
            mode: *m,
            zone_keys_needed: i as u32,
            tool: if i > 0 && *m != first { tool_for(*m).map(String::from) } else { None },
        })
        .collect();

    let per_zone = split(o.number_of_trips, &vec![1; zn]);
    let mut counters: BTreeMap<(usize, i64), i64> = BTreeMap::new();
    let mut trips: Vec<QuestSlot> = Vec::new();
    for (zi, zone) in zones.iter().enumerate() {
        // A zone's own list wins; an empty or missing one means the game's list. Walking to a point is always allowed.
        let mut quest_types: Vec<String> = o.zone_quest_types.get(zi).filter(|v| !v.is_empty()).unwrap_or(&o.quest_types).clone();
        if !quest_types.iter().any(|q| q == "reach") {
            quest_types.push("reach".into());
        }
        let counts = split(per_zone[zi], &[o.easy_share, o.medium_share, o.hard_share]);
        let mut fams: Vec<&str> = Vec::new();
        for f in FAMILIES.iter().filter(|f| quest_types.iter().any(|q| q == **f) && family_allows(f, zone.mode)) {
            fams.extend(std::iter::repeat_n(*f, if *f == "reach" { REACH_WEIGHT } else { 1 }));
        }
        for (di, n) in counts.iter().enumerate() {
            for _ in 0..*n {
                let tier = rng.random_range(BANDS[di].0..=BANDS[di].1);
                let family = fams.choose(&mut rng).copied().unwrap_or("reach");
                let block = di as i64 * 4 + mode_index(zone.mode);
                let c = counters.entry((di, block)).or_insert(0);
                *c += 1;
                trips.push(QuestSlot {
                    location_id: ID_OFFSET + block * BLOCK_SIZE + *c,
                    zone: zone.id,
                    mode: zone.mode,
                    difficulty: DIFFICULTIES[di].into(),
                    effort_tier: tier,
                    family: family.into(),
                });
            }
        }
    }
    let boss = has_boss_goal(&o.goal_list()).then(|| {
        let last = zones.last().expect("zones");
        QuestSlot {
            location_id: ID_OFFSET + 12 * BLOCK_SIZE + 1,
            zone: last.id,
            mode: last.mode,
            difficulty: "hard".into(),
            effort_tier: 10,
            family: "boss".into(),
        }
    });

    // ---- item pool (mirrors the apworld item plan) ----
    let total_locs = trips.len() as u32 + u32::from(boss.is_some());
    let mut unlock: Vec<(usize, String)> = Vec::new(); // (zone index that needs it, item)
    for k in 2..=zn {
        unlock.push((k, "Progressive Zone Key".into()));
        if let Some(t) = zones[k - 1].tool.as_deref() {
            if !unlock.iter().any(|(_, n)| n == t) {
                unlock.push((k, t.into()));
            }
        }
    }
    let letters: Vec<String> = letters_for_goals(&o.goal_list()).chars().map(|c| format!("Letter {c}")).collect();
    let mut free = i64::from(total_locs) - unlock.len() as i64 - letters.len() as i64;
    let mut other: Vec<String> = Vec::new();
    let add_useful = |name: &str, share_pct: u32, min: u32, free: &mut i64, other: &mut Vec<String>| {
        let want = ((f64::from(total_locs) * f64::from(share_pct) / 100.0).floor() as u32).max(min).min((*free).max(0) as u32);
        other.extend(std::iter::repeat_n(name.to_string(), want as usize));
        *free -= i64::from(want);
    };
    if o.enable_effort_reductions {
        add_useful("Progressive Effort Reduction", 15, 5, &mut free, &mut other);
    }
    if o.enable_scouting {
        add_useful("Progressive Scouting Distance", 5, 3, &mut free, &mut other);
    }
    if o.enable_collection {
        add_useful("Progressive Collection Distance", 5, 3, &mut free, &mut other);
    }
    let free = free.max(0) as u32;
    let mut trap_names: Vec<String> = Vec::new();
    for t in &o.enabled_traps {
        if t == "honor" {
            trap_names.extend(HONOR_TRAPS.iter().map(std::string::ToString::to_string));
        } else if let Some(n) = trap_item(t) {
            trap_names.push(n.to_string());
        }
    }
    let traps = if trap_names.is_empty() { 0 } else { free * o.trap_rate.min(100) / 100 };
    for _ in 0..traps {
        other.push(trap_names.choose(&mut rng).cloned().expect("non-empty"));
    }
    for _ in 0..(free - traps) {
        other.push(FILLERS.choose(&mut rng).map(std::string::ToString::to_string).expect("fillers"));
    }

    // ---- fill: unlock items go to zones that are already reachable, so the game is beatable by construction ----
    let mut locs: Vec<(i64, u32, bool)> = trips.iter().map(|t| (t.location_id, t.zone, false)).collect();
    if let Some(b) = &boss {
        locs.push((b.location_id, b.zone, true));
    }
    let mut rewards: BTreeMap<i64, String> = BTreeMap::new();
    let place = |item: &str, max_zone: u32, rewards: &mut BTreeMap<i64, String>, rng: &mut StdRng| -> Result<(), String> {
        let open: Vec<i64> = locs.iter().filter(|(id, z, boss)| *z <= max_zone && !*boss && !rewards.contains_key(id)).map(|(id, _, _)| *id).collect();
        let id = *open.choose(rng).ok_or("not enough quests to hold the keys and tools; add more trips")?;
        rewards.insert(id, item.to_string());
        Ok(())
    };
    for (zone_needing, item) in &unlock {
        place(item, *zone_needing as u32 - 1, &mut rewards, &mut rng)?;
    }
    for l in &letters {
        place(l, zn as u32, &mut rewards, &mut rng)?;
    }
    other.shuffle(&mut rng);
    for (id, _, _) in &locs {
        if !rewards.contains_key(id) {
            rewards.insert(*id, other.pop().ok_or("item pool smaller than the number of quests")?);
        }
    }

    let slot = SlotData {
        schema_version: CURRENT_SCHEMA,
        goal: String::new(), // schema 3 carries goals only
        goal_target: 0,
        goals: o.goal_list(),
        goal_mode: o.goal_mode,
        goal_need: o.goal_need,
        minutes_per_tier: o.minutes_per_tier,
        reduction_percent: o.reduction_percent,
        min_distance_m: o.min_distance_m,
        fog_of_war: o.fog_of_war,
        return_home: o.return_home,
        death_link: false,
        enabled_traps: o.enabled_traps.clone(),
        zones,
        trips,
        boss,
    };
    Ok(SoloGame { slot, rewards })
}

/// Simulation used by tests and the app: can every quest (and the goal items) be reached by collecting rewards?
#[must_use]
pub fn is_beatable(g: &SoloGame) -> bool {
    let mut have: BTreeMap<String, u32> = BTreeMap::new();
    let mut collected: Vec<i64> = Vec::new();
    loop {
        let mut grew = false;
        for q in g.slot.all_quests() {
            if collected.contains(&q.location_id) {
                continue;
            }
            let keys = *have.get("Progressive Zone Key").unwrap_or(&0);
            let open = g.slot.zone(q.zone).is_some_and(|z| keys >= z.zone_keys_needed && z.tool.as_ref().is_none_or(|t| have.contains_key(t)));
            if open {
                collected.push(q.location_id);
                *have.entry(g.rewards[&q.location_id].clone()).or_insert(0) += 1;
                grew = true;
            }
        }
        if !grew {
            break;
        }
    }
    collected.len() == g.slot.all_quests().len()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opts(zones: &[Mode], trips: u32, goal: &str) -> SoloOptions {
        SoloOptions { zone_modes: zones.to_vec(), number_of_trips: trips, goal: goal.into(), ..SoloOptions::default() }
    }

    #[test]
    fn several_goals_together_ask_for_everything_each_one_needs() {
        let mut o = opts(&[Mode::Walk], 60, "macguffin_short");
        o.goals = vec![GoalSpec { id: "macguffin_long".into(), target: 0 }, GoalSpec { id: "boss".into(), target: 0 }];
        o.goal_mode = GoalMode::All;
        let g = generate(&o, 3).unwrap();
        assert!(g.slot.boss.is_some(), "the boss goal needs a boss quest");
        let letters = g.rewards.values().filter(|r| r.starts_with("Letter ")).count();
        assert_eq!(letters, "ARCHIPELAGO".len(), "the long word needs all its letters");
        assert_eq!(g.slot.goals.len(), 2);
        assert_eq!(g.slot.goal_mode, GoalMode::All);
        assert_eq!(g.slot.goals[0].id, "macguffin_long");
    }

    #[test]
    fn a_goal_list_is_checked_like_a_single_goal() {
        let mut o = opts(&[Mode::Walk], 60, "boss");
        o.goals = vec![GoalSpec { id: "nonsense".into(), target: 0 }];
        assert!(validate(&o).is_err());
        o.goals = vec![GoalSpec { id: "boss".into(), target: 0 }, GoalSpec { id: "quest_dex".into(), target: 4 }];
        o.goal_mode = GoalMode::AtLeast;
        o.goal_need = 3;
        assert!(validate(&o).unwrap_err().contains("goal"), "3 of 2 goals is impossible");
        o.goal_need = 2;
        assert!(validate(&o).is_ok());
    }

    #[test]
    fn each_zone_can_have_its_own_quest_types() {
        let mut o = opts(&[Mode::Walk, Mode::Walk], 80, "all_trips");
        o.zone_quest_types = vec![vec!["park".into()], vec!["dwell".into()]];
        let g = generate(&o, 5).unwrap();
        let fams = |zone: u32| -> std::collections::BTreeSet<String> { g.slot.trips.iter().filter(|q| q.zone == zone).map(|q| q.family.clone()).collect() };
        let allowed = |zone: u32, own: &str| fams(zone).iter().all(|f| f == own || f == "reach"); // reach is always allowed
        assert!(allowed(1, "park"), "zone 1 got {:?}", fams(1));
        assert!(allowed(2, "dwell"), "zone 2 got {:?}", fams(2));
        assert!(fams(1).contains("park") && fams(2).contains("dwell"));
    }

    #[test]
    fn a_zone_with_no_quest_types_of_its_own_uses_the_games_list() {
        let mut o = opts(&[Mode::Walk, Mode::Walk], 80, "all_trips");
        o.quest_types = vec!["trail".into()];
        o.zone_quest_types = vec![vec![], vec!["dwell".into()]];
        let g = generate(&o, 5).unwrap();
        let fams = |zone: u32| -> std::collections::BTreeSet<String> { g.slot.trips.iter().filter(|q| q.zone == zone).map(|q| q.family.clone()).collect() };
        assert!(fams(1).iter().all(|f| f == "trail" || f == "reach"), "zone 1 got {:?}", fams(1));
    }

    #[test]
    fn counts_ids_and_difficulty_bands_follow_the_apworld_layout() {
        let g = generate(&opts(&[Mode::Walk, Mode::Bike, Mode::Drive], 100, "boss"), 7).unwrap();
        assert_eq!(g.slot.trips.len(), 100);
        assert!(g.slot.boss.is_some());
        let mut ids: Vec<i64> = g.slot.all_quests().iter().map(|q| q.location_id).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), 101);
        for q in &g.slot.trips {
            let di = ["easy", "medium", "hard"].iter().position(|d| *d == q.difficulty).unwrap() as i64;
            let block = (q.location_id - ID_OFFSET - 1) / BLOCK_SIZE;
            assert_eq!(block, di * 4 + mode_index(q.mode), "id {} does not match its block", q.location_id);
            assert!((BANDS[di as usize].0..=BANDS[di as usize].1).contains(&q.effort_tier));
            assert!(family_allows(&q.family, q.mode), "{} not allowed for {:?}", q.family, q.mode);
        }
        let shares: Vec<usize> = ["easy", "medium", "hard"].iter().map(|d| g.slot.trips.iter().filter(|q| q.difficulty == *d).count()).collect();
        assert_eq!(shares.iter().sum::<usize>(), 100);
        assert!(shares[0] > shares[1] && shares[1] > shares[2], "50/35/15 mix, got {shares:?}");
        assert_eq!(g.slot.zones[1].tool.as_deref(), Some("Bike"));
        assert_eq!(g.slot.zones[2].zone_keys_needed, 2);
    }

    #[test]
    fn reward_table_matches_the_pool_and_is_always_beatable() {
        for (zones, trips, goal) in [
            (vec![Mode::Walk], 20, "macguffin_short"),
            (vec![Mode::Walk, Mode::Bike], 40, "macguffin_long"),
            (vec![Mode::Walk, Mode::Run, Mode::Bike, Mode::Drive], 60, "treasure_hunt"),
            (vec![Mode::Bike, Mode::Walk, Mode::Walk], 30, "zone_conqueror"),
            (vec![Mode::Walk; 6], 80, "all_trips"),
        ] {
            for seed in 0..25 {
                let g = generate(&opts(&zones, trips, goal), seed).unwrap_or_else(|e| panic!("{zones:?} {goal} seed {seed}: {e}"));
                assert_eq!(g.rewards.len(), g.slot.all_quests().len());
                assert!(is_beatable(&g), "{zones:?} {goal} seed {seed} is not beatable");
                let count = |n: &str| g.rewards.values().filter(|v| *v == n).count();
                assert_eq!(count("Progressive Zone Key"), zones.len() - 1);
                assert_eq!(g.rewards.values().filter(|v| v.starts_with("Letter ")).count(), letters_for(goal).len());
            }
        }
    }

    #[test]
    fn unlock_items_never_sit_behind_the_zone_they_unlock() {
        let g = generate(&opts(&[Mode::Walk, Mode::Run, Mode::Bike], 30, "all_trips"), 3).unwrap();
        let zone_of = |id: i64| g.slot.all_quests().iter().find(|q| q.location_id == id).unwrap().zone;
        let keys: Vec<u32> = g.rewards.iter().filter(|(_, v)| *v == "Progressive Zone Key").map(|(k, _)| zone_of(*k)).collect();
        assert!(keys.iter().all(|z| *z < 3), "keys are in zones 1-2: {keys:?}");
        let shoes = g.rewards.iter().find(|(_, v)| *v == "Running Shoes").map(|(k, _)| zone_of(*k)).unwrap();
        assert_eq!(shoes, 1, "the first tool needed for zone 2 must be in zone 1");
    }

    #[test]
    fn options_are_validated_with_clear_errors() {
        assert!(generate(&opts(&[], 20, "boss"), 1).unwrap_err().contains("zone_modes"));
        assert!(generate(&opts(&[Mode::Walk], 3, "macguffin_long"), 1).unwrap_err().contains("too small"));
        let mut o = opts(&[Mode::Walk], 20, "all_trips");
        o.easy_share = 0;
        o.medium_share = 0;
        o.hard_share = 0;
        assert!(generate(&o, 1).unwrap_err().contains("share"));
        assert!(generate(&opts(&[Mode::Walk], 20, "nope"), 1).unwrap_err().contains("goal"));
        let mut q = opts(&[Mode::Walk], 20, "all_trips");
        q.quest_types = vec!["teleport".into()];
        assert!(generate(&q, 1).unwrap_err().contains("quest type"));
    }

    #[test]
    fn deterministic_and_round_trips_through_slot_data_json() {
        let o = opts(&[Mode::Walk, Mode::Bike], 50, "streak");
        let (a, b) = (generate(&o, 11).unwrap(), generate(&o, 11).unwrap());
        assert_eq!(a.rewards, b.rewards);
        let json = serde_json::to_string(&a.slot).unwrap();
        assert_eq!(SlotData::from_json(&json).unwrap(), a.slot);
        assert_ne!(generate(&o, 12).unwrap().rewards, a.rewards);
    }

    #[test]
    fn extreme_shares_and_big_games_work() {
        let mut o = opts(&[Mode::Walk; 6], 1000, "boss_rush");
        o.easy_share = 0;
        o.hard_share = 100;
        o.medium_share = 0;
        o.enable_effort_reductions = true;
        o.enable_scouting = true;
        o.enable_collection = true;
        let g = generate(&o, 5).unwrap();
        assert!(g.slot.trips.iter().all(|q| q.difficulty == "hard"));
        assert!(is_beatable(&g));
        assert_eq!(g.rewards.len(), 1000);
    }
}
