//! Standalone ("solo") mode: the app's own mini-randomizer. Mirrors the apworld (same slots, item pool, IDs)
//! so every feature works without an Archipelago server. See docs/superpowers/specs/2026-10-08-v1-quests-realms-design.md.

use std::collections::BTreeMap;

use rand::rngs::StdRng;
use rand::seq::{IndexedRandom, SliceRandom};
use rand::{RngExt, SeedableRng};
use serde::{Deserialize, Serialize};

use crate::catalog::Mode;
use crate::num::{count_i64, count_u32, round_u32};
use crate::slot::{check_goal_ids, GoalMode, QuestSlot, SlotData, ZoneSlot, CURRENT_SCHEMA};

/// First Archipelago id of this game; every location and item id is this plus an offset.
pub const ID_OFFSET: i64 = 8_902_400_000_000;
/// Number of ids reserved for each kind of thing (a block).
pub const BLOCK_SIZE: i64 = 1000;
/// Ids of the win conditions a game can use.
pub const GOALS: [&str; 2] = ["treasure_hunt", "all_trips"];
/// Ids of the quest families.
pub const FAMILIES: [&str; 10] = ["reach", "dwell", "landmark", "trail", "park", "water", "courier", "explore", "steps", "away"];
const DIFFICULTIES: [&str; 3] = ["easy", "medium", "hard"];
const BANDS: [(u8, u8); 3] = [(1, 3), (4, 7), (8, 10)];
const REACH_WEIGHT: usize = 3;
/// Names of the honour-system trap items, which the player carries out in person.
pub const HONOR_TRAPS: [&str; 5] = ["Push Up Trap", "Socializing Trap", "Sit Up Trap", "Jumping Jack Trap", "Touch Grass Trap"];
/// Names of the filler items.
pub const FILLERS: [&str; 2] = ["Hydrate!", "Take a Breather!"];

/// The item name of the trap with this key, if it is a known trap.
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

/// The badge (tool item) a quest in `mode` needs, if any (walking needs none); mirrors the apworld's `BADGES`.
#[must_use]
pub fn badge_for(mode: Mode) -> Option<&'static str> {
    match mode {
        Mode::Walk => None,
        Mode::Run => Some("Running Shoes"),
    }
}

fn mode_index(m: Mode) -> i64 {
    count_i64(Mode::ALL.iter().position(|x| *x == m).unwrap_or(0))
}

fn block(difficulty_index: usize, mode: Mode) -> i64 {
    count_i64(difficulty_index * Mode::ALL.len()) + mode_index(mode)
}

/// Location id of quest `number` (from 1) of difficulty `difficulty_index` in `mode`; mirrors the apworld's `quest_location_id`.
#[must_use]
pub fn quest_location_id(difficulty_index: usize, mode: Mode, number: i64) -> i64 {
    ID_OFFSET + block(difficulty_index, mode) * BLOCK_SIZE + number
}

/// Location id of the Boss Quest: the block after every difficulty and mode.
#[must_use]
pub fn boss_location_id() -> i64 {
    ID_OFFSET + count_i64(DIFFICULTIES.len() * Mode::ALL.len()) * BLOCK_SIZE + 1
}

/// The options of a solo game: the same choices as the apworld YAML.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(clippy::struct_excessive_bools)] // mirrors the YAML options, each an independent switch
pub struct SoloOptions {
    /// The win condition. When `goals` is not empty it takes over (and these two are only the single-goal form).
    pub goal: String,
    /// The win conditions; when not empty they replace `goal`.
    pub goals: Vec<String>,
    /// How the goals combine into winning.
    pub goal_mode: GoalMode,
    /// For an at-least goal mode, how many goals must be finished.
    pub goal_need: u32,
    /// How many quest locations the game has.
    pub number_of_trips: u32,
    /// How many zones (1 to 6); zone k needs k - 1 zone keys.
    pub zone_count: u32,
    /// Whether some quests are runs, which need Running Shoes.
    pub run_quests: bool,
    /// Percent of quests that are runs when `run_quests` is on (1 to 50).
    pub run_share: u32,
    /// Weight of easy quests.
    pub easy_share: u32,
    /// Weight of medium quests.
    pub medium_share: u32,
    /// Weight of hard quests.
    pub hard_share: u32,
    /// Minutes of effort one tier covers.
    pub minutes_per_tier: u32,
    /// Quests must be at least this far from home, in metres.
    pub min_distance_m: u32,
    /// Quest families the game may use.
    pub quest_types: Vec<String>,
    /// Quest types for each zone, in zone order. A missing or empty entry uses `quest_types`.
    pub zone_quest_types: Vec<Vec<String>>,
    /// Keys of the traps in the item pool.
    pub enabled_traps: Vec<String>,
    /// Share of filler items that are traps.
    pub trap_rate: u32,
    /// Whether effort-reduction items are in the pool.
    pub enable_effort_reductions: bool,
    /// Whether scout items are in the pool.
    pub enable_scouting: bool,
    /// Whether collection items are in the pool.
    pub enable_collection: bool,
    /// Percent of effort each reduction item removes.
    pub reduction_percent: u32,
    /// Whether quests stay hidden until the player is near.
    pub fog_of_war: bool,
    /// Whether the player must return home to finish a quest.
    pub return_home: bool,
}

impl Default for SoloOptions {
    fn default() -> Self {
        Self {
            goal: "treasure_hunt".into(),
            goals: vec![],
            goal_mode: GoalMode::All,
            goal_need: 0,
            number_of_trips: 100,
            zone_count: 1,
            run_quests: false,
            run_share: 20,
            easy_share: 50,
            medium_share: 35,
            hard_share: 15,
            minutes_per_tier: 10,
            min_distance_m: 150,
            quest_types: FAMILIES.iter().map(ToString::to_string).collect(),
            zone_quest_types: vec![],
            enabled_traps: ["freeze", "fog", "shuffle", "silence", "leash", "detour", "toll", "slow", "honor"].iter().map(ToString::to_string).collect(),
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

/// A generated solo game: its slot data and where every reward is.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SoloGame {
    /// The generated `slot_data`.
    pub slot: SlotData,
    /// `location_id` -> item name found there.
    pub rewards: BTreeMap<i64, String>,
}

/// Treasure Hunt is the one goal with letters (A, P, G, O) and a Boss Quest.
fn has_treasure_hunt(goals: &[String]) -> bool {
    goals.iter().any(|g| g == "treasure_hunt")
}

/// The letters a set of goals needs.
fn letters_for_goals(goals: &[String]) -> &'static str {
    if has_treasure_hunt(goals) {
        "APGO"
    } else {
        ""
    }
}

impl SoloOptions {
    /// The win conditions: the `goals` list, or the single `goal`.
    #[must_use]
    pub fn goal_list(&self) -> Vec<String> {
        if self.goals.is_empty() {
            vec![self.goal.clone()]
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
    let mut out: Vec<u32> = exact.iter().map(|e| round_u32(e.floor())).collect();
    let mut rem: Vec<usize> = (0..weights.len()).collect();
    rem.sort_by(|&a, &b| (exact[b] - exact[b].floor()).total_cmp(&(exact[a] - exact[a].floor())));
    for i in rem.into_iter().take((total - out.iter().sum::<u32>()) as usize) {
        out[i] += 1;
    }
    out
}

/// Check that `o` describes a game that can be generated.
///
/// # Errors
/// Returns a message if the zone count, run share, goal setup, difficulty shares, trip count or quest types are invalid, or there are too few
/// trips for the mandatory items.
pub fn validate(o: &SoloOptions) -> Result<(), String> {
    if !(1..=6).contains(&o.zone_count) {
        return Err(format!("zone_count must be 1 to 6, got {}", o.zone_count));
    }
    if !(1..=50).contains(&o.run_share) {
        return Err(format!("run_share must be 1 to 50, got {}", o.run_share));
    }
    check_goal_ids(&o.goal_list(), o.goal_mode, o.goal_need)?;
    if o.easy_share + o.medium_share + o.hard_share == 0 {
        return Err("at least one difficulty share must be above 0".into());
    }
    if !(1..=1000).contains(&o.number_of_trips) {
        return Err("number_of_trips must be 1..1000".into());
    }
    let zones = o.zone_count;
    let tools = count_u32(tool_names(o).len());
    let mandatory = count_u32(letters_for_goals(&o.goal_list()).len()) + (zones - 1) + tools;
    if o.number_of_trips < mandatory.max(zones) {
        return Err(format!("number_of_trips ({}) is too small; this setup needs at least {}", o.number_of_trips, mandatory.max(zones)));
    }
    if let Some(bad) = o.quest_types.iter().chain(o.zone_quest_types.iter().flatten()).find(|q| !FAMILIES.contains(&q.as_str())) {
        return Err(format!("unknown quest type {bad}"));
    }
    Ok(())
}

/// The badge items the pool holds: Running Shoes when run quests are on.
#[must_use]
pub fn tool_names(o: &SoloOptions) -> Vec<&'static str> {
    if o.run_quests {
        badge_for(Mode::Run).into_iter().collect()
    } else {
        Vec::new()
    }
}

/// How many of `trips` quests are runs for `share` percent (half up, at least 1 when on); mirrors the apworld's `run_count`.
///
/// Capped so the first quest of every zone stays open (walk, no badge).
#[must_use]
pub fn run_count(trips: u32, zones: u32, share: u32) -> u32 {
    if share == 0 {
        return 0;
    }
    ((trips * share + 50) / 100).max(1).min(trips.saturating_sub(zones))
}

/// Generate a solo game from `o`: the zones, quest locations and what each location gives.
///
/// The same `o` and `seed` always give the same game.
///
/// # Errors
/// Returns a message if `o` is invalid (see [`validate`]) or there are too few quests to hold the keys and tools.
#[allow(clippy::too_many_lines)] // one linear pipeline; splitting it would scatter the RNG draw order that keeps seeds reproducible
pub fn generate(o: &SoloOptions, seed: u64) -> Result<SoloGame, String> {
    validate(o)?;
    let mut rng = StdRng::seed_from_u64(seed);
    let zn = o.zone_count as usize;

    let zones: Vec<ZoneSlot> = (0..o.zone_count).map(|i| ZoneSlot { id: i + 1, zone_keys_needed: i }).collect();

    // ---- quest slots: zone, difficulty, tier and family; the mode is picked after, as in the apworld ----
    let per_zone = split(o.number_of_trips, &vec![1; zn]);
    let mut slots: Vec<(u32, usize, u8, &str)> = Vec::new();
    let mut firsts: Vec<usize> = Vec::new();
    for (zi, zone) in zones.iter().enumerate() {
        // A zone's own list wins; an empty or missing one means the game's list. Walking to a point is always allowed.
        let mut quest_types: Vec<String> = o.zone_quest_types.get(zi).filter(|v| !v.is_empty()).unwrap_or(&o.quest_types).clone();
        if !quest_types.iter().any(|q| q == "reach") {
            quest_types.push("reach".into());
        }
        let counts = split(per_zone[zi], &[o.easy_share, o.medium_share, o.hard_share]);
        let mut fams: Vec<&str> = Vec::new();
        for f in FAMILIES.iter().filter(|f| quest_types.iter().any(|q| q == **f)) {
            fams.extend(std::iter::repeat_n(*f, if *f == "reach" { REACH_WEIGHT } else { 1 }));
        }
        firsts.push(slots.len());
        for (di, n) in counts.iter().enumerate() {
            for _ in 0..*n {
                let tier = rng.random_range(BANDS[di].0..=BANDS[di].1);
                let family = fams.choose(&mut rng).copied().unwrap_or("reach");
                slots.push((zone.id, di, tier, family));
            }
        }
    }
    let share = if o.run_quests { o.run_share } else { 0 };
    let candidates: Vec<usize> = (0..slots.len()).filter(|i| !firsts.contains(i)).collect();
    let runs: Vec<usize> = candidates.sample(&mut rng, run_count(o.number_of_trips, o.zone_count, share) as usize).copied().collect();

    let mut counters: BTreeMap<(usize, i64), i64> = BTreeMap::new();
    let mut trips: Vec<QuestSlot> = Vec::new();
    for (i, (zone, di, tier, family)) in slots.into_iter().enumerate() {
        let mode = if runs.contains(&i) { Mode::Run } else { Mode::Walk };
        let c = counters.entry((di, block(di, mode))).or_insert(0);
        *c += 1;
        trips.push(QuestSlot {
            location_id: quest_location_id(di, mode, *c),
            zone,
            mode,
            difficulty: DIFFICULTIES[di].into(),
            effort_tier: tier,
            family: family.into(),
            needs: badge_for(mode).map(String::from).into_iter().collect(),
        });
    }
    let boss = zones.last().filter(|_| has_treasure_hunt(&o.goal_list())).map(|last| QuestSlot {
        location_id: boss_location_id(),
        zone: last.id,
        mode: Mode::Walk,
        difficulty: "hard".into(),
        effort_tier: 10,
        family: "boss".into(),
        needs: vec![],
    });

    // ---- item pool (mirrors the apworld item plan) ----
    let total_locs = count_u32(trips.len()) + u32::from(boss.is_some());
    // (zone index that needs it, item); the badge is needed from zone 1 on, so it goes after the keys anywhere open.
    let mut unlock: Vec<(usize, String)> = (2..=zn).map(|k| (k, "Progressive Zone Key".to_string())).collect();
    if trips.iter().any(|t| !t.needs.is_empty()) {
        unlock.extend(tool_names(o).into_iter().map(|t| (zn + 1, t.to_string())));
    }
    let letters: Vec<String> = letters_for_goals(&o.goal_list()).chars().map(|c| format!("Letter {c}")).collect();
    let mut free = i64::from(total_locs) - count_i64(unlock.len()) - count_i64(letters.len());
    let mut other: Vec<String> = Vec::new();
    let add_useful = |name: &str, share_pct: u32, min: u32, free: &mut i64, other: &mut Vec<String>| {
        let want = round_u32((f64::from(total_locs) * f64::from(share_pct) / 100.0).floor()).max(min).min(u32::try_from((*free).max(0)).unwrap_or(u32::MAX));
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
    let free = u32::try_from(free.max(0)).unwrap_or(u32::MAX);
    let mut trap_names: Vec<String> = Vec::new();
    for t in &o.enabled_traps {
        if t == "honor" {
            trap_names.extend(HONOR_TRAPS.iter().map(ToString::to_string));
        } else if let Some(n) = trap_item(t) {
            trap_names.push(n.to_string());
        }
    }
    let n_traps = if trap_names.is_empty() { 0 } else { free * o.trap_rate.min(100) / 100 };
    for _ in 0..n_traps {
        if let Some(name) = trap_names.choose(&mut rng) {
            other.push(name.clone());
        }
    }
    for _ in 0..(free - n_traps) {
        if let Some(name) = FILLERS.choose(&mut rng) {
            other.push((*name).to_string());
        }
    }

    // ---- fill: unlock items go to open quests (no badge) of zones already reachable, so the game is beatable by construction ----
    // (location id, zone, may hold an unlock item: not the boss and no badge)
    let mut locs: Vec<(i64, u32, bool)> = trips.iter().map(|t| (t.location_id, t.zone, t.needs.is_empty())).collect();
    if let Some(b) = &boss {
        locs.push((b.location_id, b.zone, false));
    }
    let mut rewards: BTreeMap<i64, String> = BTreeMap::new();
    let place = |item: &str, max_zone: u32, unlock: bool, rewards: &mut BTreeMap<i64, String>, rng: &mut StdRng| -> Result<(), String> {
        let open: Vec<i64> = locs
            .iter()
            .filter(|(id, z, holds)| *z <= max_zone && (*holds || !unlock) && !rewards.contains_key(id) && *id != boss_location_id())
            .map(|(id, _, _)| *id)
            .collect();
        let id = *open.choose(rng).ok_or("not enough quests to hold the keys and tools; add more trips")?;
        rewards.insert(id, item.to_string());
        Ok(())
    };
    for (zone_needing, item) in &unlock {
        place(item, count_u32(*zone_needing) - 1, true, &mut rewards, &mut rng)?;
    }
    for l in &letters {
        place(l, count_u32(zn), false, &mut rewards, &mut rng)?;
    }
    other.shuffle(&mut rng);
    for (id, _, _) in &locs {
        if !rewards.contains_key(id) {
            rewards.insert(*id, other.pop().ok_or("item pool smaller than the number of quests")?);
        }
    }

    let slot = SlotData {
        schema_version: CURRENT_SCHEMA,
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
            let open = g.slot.zone(q.zone).is_some_and(|z| keys >= z.zone_keys_needed) && q.needs.iter().all(|n| have.contains_key(n));
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
#[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap, clippy::cast_sign_loss)] // test code: test fixtures use small numbers; test fixtures use small, known-positive numbers
mod tests {
    use super::*;

    fn opts(zones: u32, trips: u32, goal: &str) -> SoloOptions {
        SoloOptions { zone_count: zones, number_of_trips: trips, goal: goal.into(), ..SoloOptions::default() }
    }

    /// `opts` with run quests on at `share` percent.
    fn runs(zones: u32, trips: u32, goal: &str, share: u32) -> SoloOptions {
        SoloOptions { run_quests: true, run_share: share, ..opts(zones, trips, goal) }
    }

    #[test]
    fn only_the_two_beta_goals_exist_and_treasure_hunt_is_the_default() {
        assert_eq!(GOALS, ["treasure_hunt", "all_trips"]);
        assert_eq!(SoloOptions::default().goal, "treasure_hunt");
        let g = generate(&SoloOptions::default(), 1).unwrap();
        assert!(g.slot.boss.is_some(), "Treasure Hunt needs its Boss Quest");
        assert_eq!(g.rewards.values().filter(|r| r.starts_with("Letter ")).count(), 4);
        assert!(is_beatable(&g));
    }

    #[test]
    fn several_goals_together_ask_for_everything_each_one_needs() {
        let mut o = opts(1, 60, "all_trips");
        o.goals = vec!["treasure_hunt".to_string(), "all_trips".to_string()];
        o.goal_mode = GoalMode::All;
        let g = generate(&o, 3).unwrap();
        assert!(g.slot.boss.is_some(), "Treasure Hunt needs a boss quest");
        let letters = g.rewards.values().filter(|r| r.starts_with("Letter ")).count();
        assert_eq!(letters, "APGO".len(), "Treasure Hunt needs its letters");
        assert_eq!(g.slot.goals.len(), 2);
        assert_eq!(g.slot.goal_mode, GoalMode::All);
        assert_eq!(g.slot.goals[0], "treasure_hunt");
    }

    #[test]
    fn a_goal_list_is_checked_like_a_single_goal() {
        let mut o = opts(1, 60, "treasure_hunt");
        o.goals = vec!["nonsense".to_string()];
        assert!(validate(&o).is_err());
        o.goals = vec!["treasure_hunt".to_string(), "all_trips".to_string()];
        o.goal_mode = GoalMode::AtLeast;
        o.goal_need = 3;
        assert!(validate(&o).unwrap_err().contains("goal"), "3 of 2 goals is impossible");
        o.goal_need = 2;
        assert!(validate(&o).is_ok());
    }

    #[test]
    fn each_zone_can_have_its_own_quest_types() {
        let mut o = opts(2, 80, "all_trips");
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
        let mut o = opts(2, 80, "all_trips");
        o.quest_types = vec!["trail".into()];
        o.zone_quest_types = vec![vec![], vec!["dwell".into()]];
        let g = generate(&o, 5).unwrap();
        let fams = |zone: u32| -> std::collections::BTreeSet<String> { g.slot.trips.iter().filter(|q| q.zone == zone).map(|q| q.family.clone()).collect() };
        assert!(fams(1).iter().all(|f| f == "trail" || f == "reach"), "zone 1 got {:?}", fams(1));
    }

    // The location id table. apworld/tests/test_locations.py pins the same rows; change both together.
    #[test]
    fn location_ids_match_the_apworld_table() {
        let table: [(usize, Mode, i64, i64); 6] = [
            (0, Mode::Walk, 1, 1),
            (0, Mode::Run, 7, 1007),
            (1, Mode::Walk, 1, 2001),
            (1, Mode::Run, 3, 3003),
            (2, Mode::Walk, 5, 4005),
            (2, Mode::Run, 1000, 6000),
        ];
        for (di, mode, n, offset) in table {
            assert_eq!(quest_location_id(di, mode, n), ID_OFFSET + offset, "{di} {mode:?} {n}");
        }
        assert_eq!(boss_location_id(), ID_OFFSET + 6001);
    }

    #[test]
    fn run_quests_need_running_shoes_and_walking_needs_nothing() {
        assert_eq!(badge_for(Mode::Walk), None);
        assert_eq!(badge_for(Mode::Run), Some("Running Shoes"));
        let g = generate(&runs(2, 40, "treasure_hunt", 25), 2).unwrap();
        let run: Vec<&QuestSlot> = g.slot.trips.iter().filter(|q| q.mode == Mode::Run).collect();
        assert_eq!(run.len(), 10);
        assert!(run.iter().all(|q| q.needs == ["Running Shoes"]));
        assert!(g.slot.trips.iter().filter(|q| q.mode == Mode::Walk).all(|q| q.needs.is_empty()));
        assert_eq!(g.slot.boss.as_ref().unwrap().location_id, boss_location_id());
        assert!(g.slot.boss.as_ref().unwrap().needs.is_empty() && g.slot.boss.as_ref().unwrap().mode == Mode::Walk);
        assert_eq!(g.rewards.values().filter(|v| *v == "Running Shoes").count(), 1);
        assert!(is_beatable(&g));
    }

    #[test]
    fn run_quests_are_off_by_default_and_then_need_no_shoes() {
        let g = generate(&opts(3, 60, "all_trips"), 4).unwrap();
        assert!(g.slot.trips.iter().all(|q| q.mode == Mode::Walk && q.needs.is_empty()));
        assert!(!g.rewards.values().any(|v| v == "Running Shoes"));
    }

    #[test]
    fn run_count_mirrors_the_apworld() {
        assert_eq!(run_count(100, 1, 0), 0);
        assert_eq!(run_count(100, 1, 20), 20);
        assert_eq!(run_count(10, 1, 25), 3, "2.5 rounds up");
        assert_eq!(run_count(10, 1, 1), 1, "never zero when on");
        assert_eq!(run_count(2, 1, 50), 1);
        assert_eq!(run_count(3, 3, 50), 0, "one quest per zone: all stay open");
        assert_eq!(run_count(7, 6, 50), 1);
    }

    #[test]
    fn the_first_quest_of_every_zone_stays_open() {
        for seed in 0..30 {
            let g = generate(&runs(6, 12, "all_trips", 50), seed).unwrap();
            for z in 1..=6 {
                let first = g.slot.trips.iter().find(|q| q.zone == z).unwrap();
                assert!(first.needs.is_empty(), "seed {seed} zone {z}");
            }
        }
    }

    #[test]
    fn counts_ids_and_difficulty_bands_follow_the_apworld_layout() {
        let g = generate(&runs(3, 100, "treasure_hunt", 20), 7).unwrap();
        assert_eq!(g.slot.trips.len(), 100);
        assert!(g.slot.boss.is_some());
        let mut ids: Vec<i64> = g.slot.all_quests().iter().map(|q| q.location_id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), 101);
        for q in &g.slot.trips {
            let di = ["easy", "medium", "hard"].iter().position(|d| *d == q.difficulty).unwrap() as i64;
            let number = (q.location_id - ID_OFFSET - 1) % BLOCK_SIZE + 1;
            assert_eq!(q.location_id, quest_location_id(di as usize, q.mode, number), "id {} does not match its block", q.location_id);
            assert!((BANDS[di as usize].0..=BANDS[di as usize].1).contains(&q.effort_tier));
            assert!(FAMILIES.contains(&q.family.as_str()), "unknown family {}", q.family);
        }
        let shares: Vec<usize> = ["easy", "medium", "hard"].iter().map(|d| g.slot.trips.iter().filter(|q| q.difficulty == *d).count()).collect();
        assert_eq!(shares.iter().sum::<usize>(), 100);
        assert!(shares[0] > shares[1] && shares[1] > shares[2], "50/35/15 mix, got {shares:?}");
        assert_eq!(g.slot.trips.iter().filter(|q| q.mode == Mode::Run).count(), 20);
        assert_eq!(g.slot.zones[2].zone_keys_needed, 2);
    }

    #[test]
    fn reward_table_matches_the_pool_and_is_always_beatable() {
        for (o, label) in [
            (opts(1, 20, "treasure_hunt"), "1 zone"),
            (runs(2, 40, "treasure_hunt", 20), "2 zones, runs"),
            (runs(4, 60, "treasure_hunt", 50), "4 zones, half runs"),
            (runs(6, 10, "treasure_hunt", 50), "6 zones, smallest"),
            (opts(3, 30, "all_trips"), "3 zones"),
            (opts(6, 80, "all_trips"), "6 zones"),
        ] {
            for seed in 0..25 {
                let g = generate(&o, seed).unwrap_or_else(|e| panic!("{label} seed {seed}: {e}"));
                assert_eq!(g.rewards.len(), g.slot.all_quests().len());
                assert!(is_beatable(&g), "{label} seed {seed} is not beatable");
                let count = |n: &str| g.rewards.values().filter(|v| *v == n).count();
                assert_eq!(count("Progressive Zone Key"), o.zone_count as usize - 1);
                assert_eq!(g.rewards.values().filter(|v| v.starts_with("Letter ")).count(), letters_for_goals(&o.goal_list()).len());
            }
        }
    }

    #[test]
    fn unlock_items_sit_on_open_quests_before_the_zone_they_unlock() {
        let g = generate(&runs(3, 30, "all_trips", 50), 3).unwrap();
        let quest = |id: i64| *g.slot.all_quests().iter().find(|q| q.location_id == id).unwrap();
        let keys: Vec<&QuestSlot> = g.rewards.iter().filter(|(_, v)| *v == "Progressive Zone Key").map(|(k, _)| quest(*k)).collect();
        assert!(keys.iter().all(|q| q.zone < 3 && q.needs.is_empty()), "keys are on open quests in zones 1-2");
        let shoes = g.rewards.iter().find(|(_, v)| *v == "Running Shoes").map(|(k, _)| quest(*k)).unwrap();
        assert!(shoes.needs.is_empty(), "the shoes are never behind themselves");
    }

    #[test]
    fn options_are_validated_with_clear_errors() {
        assert!(generate(&opts(0, 20, "treasure_hunt"), 1).unwrap_err().contains("zone_count"));
        assert!(generate(&opts(7, 20, "treasure_hunt"), 1).unwrap_err().contains("zone_count"));
        assert!(generate(&runs(1, 20, "all_trips", 51), 1).unwrap_err().contains("run_share"));
        assert!(generate(&opts(1, 3, "treasure_hunt"), 1).unwrap_err().contains("too small"));
        let mut o = opts(1, 20, "all_trips");
        o.easy_share = 0;
        o.medium_share = 0;
        o.hard_share = 0;
        assert!(generate(&o, 1).unwrap_err().contains("share"));
        assert!(generate(&opts(1, 20, "nope"), 1).unwrap_err().contains("goal"));
        let mut q = opts(1, 20, "all_trips");
        q.quest_types = vec!["teleport".into()];
        assert!(generate(&q, 1).unwrap_err().contains("quest type"));
    }

    #[test]
    fn deterministic_and_round_trips_through_slot_data_json() {
        let o = runs(2, 50, "all_trips", 20);
        let (a, b) = (generate(&o, 11).unwrap(), generate(&o, 11).unwrap());
        assert_eq!(a.rewards, b.rewards);
        let json = serde_json::to_string(&a.slot).unwrap();
        assert_eq!(SlotData::from_json(&json).unwrap(), a.slot);
        assert_ne!(generate(&o, 12).unwrap().rewards, a.rewards);
    }

    #[test]
    fn extreme_shares_and_big_games_work() {
        let mut o = opts(6, 1000, "all_trips");
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
