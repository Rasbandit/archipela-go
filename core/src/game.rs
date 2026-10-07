//! A running game: assignments, progress, rewards, fog, traps, goal. Backend is either Solo (local reward table)
//! or Archipelago (checks go to the server, items come back). Everything else is identical.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use rand::rngs::StdRng;
use rand::SeedableRng;
use serde::{Deserialize, Serialize};

use crate::assign::{assign, AssignParams, Assignment, SlotIn, SurfacePref, Target, ZoneCtx};
use crate::catalog::{Catalog, Mode};
use crate::fog::{anchor, reveal_radius, Fog};
use crate::geo::{distance_m, Point};
use crate::goal::{evaluate, GoalCtx, GoalStatus};
use crate::realm::Realm;
use crate::scan::Atlas;
use crate::slot::SlotData;
use crate::traps::Traps;
use crate::verify::{Fix, Status, Tracker, MAX_ACCURACY_M};

const DAY_MS: i64 = 86_400_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Backend {
    Solo,
    Archipelago,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuestState {
    Locked,
    Hidden,
    Open,
    InProgress,
    Done,
}

#[derive(Debug, Clone)]
pub struct QuestView {
    pub location_id: i64,
    pub zone: u32,
    pub name: String,
    pub place: String,
    pub family: String,
    /// The quest kind's id ("bench_warmer"), for choosing an icon.
    pub kind_id: String,
    pub difficulty: String,
    pub tier: u8,
    pub effort_min: f64,
    pub mode: Mode,
    pub state: QuestState,
    pub progress: f32,
    pub anchor: Option<Point>,
    pub target: Target,
    pub fallback: bool,
    pub boss: bool,
    pub blurb: String,
    /// Solo only: what the quest gave you (shown after it is done).
    pub reward: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    QuestDone { location_id: i64, name: String },
    SendCheck { location_id: i64 },
    Reward { location_id: i64, item: String },
    ZoneUnlocked { zone: u32 },
    Trap { item: String, message: String },
    ShuffleRequested,
    Discovered { location_id: i64 },
    GoalAchieved { label: String },
    Info { text: String },
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Stats {
    pub distance_m: f64,
    pub quest_days: BTreeSet<i64>,
}

#[derive(Serialize, Deserialize)]
pub struct Game {
    pub id: String,
    pub name: String,
    pub backend: Backend,
    pub seed_name: String,
    pub slot: SlotData,
    pub zone_realms: Vec<String>,
    pub home: Point,
    pub assignments: Vec<Assignment>,
    pub done: BTreeSet<i64>,
    pub items: Vec<String>,
    pub solo_rewards: BTreeMap<i64, String>,
    pub fog: Fog,
    pub traps: Traps,
    pub stats: Stats,
    pub goal_reported: bool,
    pub trap_pool: Vec<Point>,
    pub seed: u64,
    #[serde(default)]
    pub surface: SurfacePref,
    #[serde(default)]
    pub avoid_stairs: bool,
    #[serde(skip)]
    trackers: BTreeMap<i64, Tracker>,
    #[serde(skip)]
    last_fix: Option<Fix>,
    #[serde(skip)]
    last_block: Option<String>,
}

pub struct NewGame<'a> {
    pub id: String,
    pub name: String,
    pub backend: Backend,
    pub seed_name: String,
    pub slot: SlotData,
    pub zone_realms: Vec<String>,
    pub realms: &'a [(Realm, Atlas)],
    pub home: Point,
    pub seed: u64,
    pub solo_rewards: BTreeMap<i64, String>,
    pub surface: SurfacePref,
    pub avoid_stairs: bool,
}

fn speed_ok(mode: Mode, kmh: f64) -> bool {
    match mode {
        Mode::Walk => kmh <= 12.0,
        Mode::Run => kmh <= 25.0,
        Mode::Bike => kmh <= 50.0,
        Mode::Drive => true,
    }
}

fn streak(days: &BTreeSet<i64>, today: i64) -> u32 {
    let mut d = if days.contains(&today) { today } else { today - 1 };
    let mut n = 0;
    while days.contains(&d) {
        n += 1;
        d -= 1;
    }
    n
}

fn zone_ctx<'a>(slot: &SlotData, zone_realms: &[String], realms: &'a [(Realm, Atlas)]) -> Result<Vec<ZoneCtx<'a>>, String> {
    let mut out = Vec::new();
    for (i, z) in slot.zones.iter().enumerate() {
        let id = zone_realms.get(i).ok_or_else(|| format!("zone {} has no realm assigned", z.id))?;
        let (realm, atlas) = realms.iter().find(|(r, _)| &r.id == id).map(|(r, a)| (r, a)).ok_or_else(|| format!("realm {id} not found"))?;
        if realm.mode != z.mode {
            return Err(format!("zone {} is a {} zone but realm \"{}\" is tagged {}", z.id, z.mode.name(), realm.name, realm.mode.name()));
        }
        out.push(ZoneCtx { zone: z.id, mode: z.mode, realm, atlas });
    }
    Ok(out)
}

fn slots_in(slot: &SlotData, only: Option<&[i64]>) -> Vec<SlotIn> {
    slot.all_quests()
        .into_iter()
        .filter(|q| only.is_none_or(|ids| ids.contains(&q.location_id)))
        .map(|q| SlotIn { location_id: q.location_id, zone: q.zone, mode: q.mode, family: q.family.clone(), tier: q.effort_tier, boss: q.family == "boss" })
        .collect()
}

impl Game {
    pub fn create(n: NewGame, catalog: &Catalog) -> Result<Game, String> {
        let zones = zone_ctx(&n.slot, &n.zone_realms, n.realms)?;
        let params = AssignParams {
            home: n.home,
            minutes_per_tier: f64::from(n.slot.minutes_per_tier),
            min_distance_m: f64::from(n.slot.min_distance_m),
            seed: n.seed,
            surface: n.surface,
            avoid_stairs: n.avoid_stairs,
        };
        let assignments = assign(&slots_in(&n.slot, None), &zones, catalog, &params);
        let pool: Vec<Point> =
            zones.first().map(|z| z.atlas.streets.iter().step_by((z.atlas.streets.len() / 600).max(1)).copied().collect()).unwrap_or_default();
        Ok(Game {
            id: n.id,
            name: n.name,
            backend: n.backend,
            seed_name: n.seed_name,
            slot: n.slot,
            zone_realms: n.zone_realms,
            home: n.home,
            assignments,
            done: BTreeSet::new(),
            items: Vec::new(),
            solo_rewards: n.solo_rewards,
            fog: Fog::default(),
            traps: Traps::default(),
            stats: Stats::default(),
            goal_reported: false,
            trap_pool: pool,
            seed: n.seed,
            surface: n.surface,
            avoid_stairs: n.avoid_stairs,
            trackers: BTreeMap::new(),
            last_fix: None,
            last_block: None,
        })
    }

    fn count(&self, item: &str) -> u32 {
        self.items.iter().filter(|i| *i == item).count() as u32
    }

    pub fn zone_unlocked(&self, zone: u32) -> bool {
        let keys = self.count("Progressive Zone Key");
        self.slot.zone(zone).is_some_and(|z| keys >= z.zone_keys_needed && z.tool.as_ref().is_none_or(|t| self.items.contains(t)))
    }

    fn unlocked_set(&self) -> BTreeSet<u32> {
        self.slot.zones.iter().map(|z| z.id).filter(|z| self.zone_unlocked(*z)).collect()
    }

    pub fn goal_status(&self, now_ms: i64) -> GoalStatus {
        evaluate(&GoalCtx {
            slot: &self.slot,
            assignments: &self.assignments,
            done: &self.done,
            items: &self.items,
            distance_m: self.stats.distance_m,
            cells_discovered: self.fog.cells.len(),
            streak_days: streak(&self.stats.quest_days, now_ms / DAY_MS),
        })
    }

    fn fog_on(&self) -> bool {
        self.slot.fog_of_war
    }

    pub fn quest_views(&self) -> Vec<QuestView> {
        self.assignments
            .iter()
            .map(|a| {
                let done = self.done.contains(&a.location_id);
                let hidden = (self.fog_on() && !self.fog.discovered.contains(&a.location_id)) || (self.traps.fog_active() && !done);
                let progress = self.trackers.get(&a.location_id).map_or(0.0, |t| match t.status() {
                    Status::Active(p) => p,
                    Status::Done => 1.0,
                    Status::Idle => 0.0,
                });
                let state = if done {
                    QuestState::Done
                } else if !self.zone_unlocked(a.zone) {
                    QuestState::Locked
                } else if hidden {
                    QuestState::Hidden
                } else if progress > 0.0 {
                    QuestState::InProgress
                } else {
                    QuestState::Open
                };
                let difficulty = self.slot.all_quests().iter().find(|q| q.location_id == a.location_id).map(|q| q.difficulty.clone()).unwrap_or_default();
                QuestView {
                    location_id: a.location_id,
                    zone: a.zone,
                    name: a.quest_name.clone(),
                    place: a.place.clone(),
                    family: a.family.clone(),
                    kind_id: a.kind_id.clone(),
                    difficulty,
                    tier: a.tier,
                    effort_min: a.effort_min,
                    mode: a.mode,
                    state,
                    progress,
                    anchor: anchor(&a.target),
                    target: a.target.clone(),
                    fallback: a.fallback,
                    boss: a.boss,
                    blurb: a.blurb.clone(),
                    reward: if done { self.solo_rewards.get(&a.location_id).cloned() } else { None },
                }
            })
            .collect()
    }

    fn adjusted(&self, t: &Target) -> Target {
        let m = self.traps.dwell_multiplier();
        match t {
            Target::Dwell { p, r, minutes } => Target::Dwell { p: *p, r: *r, minutes: minutes * m },
            Target::DwellArea { poly, center, r, minutes } => Target::DwellArea { poly: poly.clone(), center: *center, r: *r, minutes: minutes * m },
            other => other.clone(),
        }
    }

    /// Feed a GPS fix (and the cumulative step counter if the phone has one).
    pub fn on_fix(&mut self, fix: Fix, steps_total: Option<i64>) -> Vec<Event> {
        let mut ev = Vec::new();
        if fix.accuracy_m > MAX_ACCURACY_M {
            return ev;
        }
        let pos = fix.point();
        let (mut moved, mut speed) = (0.0, None);
        if let Some(l) = self.last_fix {
            let dt = (fix.t_ms - l.t_ms) as f64 / 1000.0;
            if dt >= 3.0 {
                let d = distance_m(l.point(), pos);
                let kmh = d / dt * 3.6;
                if dt <= 120.0 {
                    speed = Some(kmh);
                }
                if kmh <= 150.0 && dt <= 300.0 {
                    moved = d;
                }
            }
        }
        self.stats.distance_m += moved;

        let scout = self.count("Progressive Scouting Distance");
        for id in self.fog.update(pos, &self.assignments, reveal_radius(scout)) {
            if self.fog_on() {
                ev.push(Event::Discovered { location_id: id });
            }
        }
        for text in self.traps.tick(fix.t_ms, pos, moved) {
            ev.push(Event::Info { text });
        }
        let blocked = self.traps.blocks_checks(pos);
        if blocked != self.last_block {
            if let Some(b) = &blocked {
                ev.push(Event::Info { text: b.clone() });
            }
            self.last_block = blocked.clone();
        }

        let mut finished = Vec::new();
        if blocked.is_none() {
            let ids: Vec<(i64, Mode, u32)> = self.assignments.iter().map(|a| (a.location_id, a.mode, a.zone)).collect();
            for (id, mode, zone) in ids {
                if self.done.contains(&id) || !self.zone_unlocked(zone) {
                    continue;
                }
                if self.fog_on() && !self.fog.discovered.contains(&id) {
                    continue;
                }
                if speed.is_some_and(|s| !speed_ok(mode, s)) {
                    continue;
                }
                if !self.trackers.contains_key(&id) {
                    let Some(a) = self.assignments.iter().find(|a| a.location_id == id) else { continue };
                    let t = Tracker::new(self.adjusted(&a.target), self.home);
                    self.trackers.insert(id, t);
                }
                if let Some(t) = self.trackers.get_mut(&id) {
                    if t.update(&fix, steps_total) == Status::Done {
                        finished.push(id);
                    }
                }
            }
        }
        for id in finished {
            ev.extend(self.complete(id, fix.t_ms, Some(pos)));
        }
        self.last_fix = Some(fix);
        ev
    }

    fn complete(&mut self, id: i64, now_ms: i64, pos: Option<Point>) -> Vec<Event> {
        let mut ev = Vec::new();
        if !self.done.insert(id) {
            return ev;
        }
        self.trackers.remove(&id);
        self.stats.quest_days.insert(now_ms / DAY_MS);
        let name = self.assignments.iter().find(|a| a.location_id == id).map(|a| a.quest_name.clone()).unwrap_or_default();
        ev.push(Event::QuestDone { location_id: id, name });
        match self.backend {
            Backend::Solo => {
                if let Some(item) = self.solo_rewards.get(&id).cloned() {
                    ev.push(Event::Reward { location_id: id, item: item.clone() });
                    ev.extend(self.receive_item(&item, now_ms, pos));
                }
            }
            Backend::Archipelago => ev.push(Event::SendCheck { location_id: id }),
        }
        ev.extend(self.check_goal(now_ms));
        ev
    }

    fn check_goal(&mut self, now_ms: i64) -> Vec<Event> {
        if !self.goal_reported {
            let s = self.goal_status(now_ms);
            if s.achieved {
                self.goal_reported = true;
                return vec![Event::GoalAchieved { label: s.label }];
            }
        }
        vec![]
    }

    /// An item arrived (solo reward or server). Applies unlocks and trap effects.
    pub fn receive_item(&mut self, name: &str, now_ms: i64, pos: Option<Point>) -> Vec<Event> {
        let before = self.unlocked_set();
        self.items.push(name.to_string());
        let mut ev = Vec::new();
        for z in self.unlocked_set().difference(&before) {
            ev.push(Event::ZoneUnlocked { zone: *z });
        }
        if name.ends_with("Trap") {
            let mut rng = StdRng::seed_from_u64(self.seed ^ (self.items.len() as u64).wrapping_mul(0x9E37_79B9));
            if let Some(message) = self.traps.trigger(name, now_ms, pos, self.home, &self.trap_pool, &mut rng) {
                ev.push(Event::Trap { item: name.to_string(), message });
            }
            if name == "Shuffle Trap" {
                ev.push(Event::ShuffleRequested);
            }
        }
        ev.extend(self.check_goal(now_ms));
        ev
    }

    /// Archipelago: replace the received-item list with the server's; only NEW items trigger effects.
    pub fn sync_items(&mut self, all: &[String], now_ms: i64, pos: Option<Point>) -> Vec<Event> {
        if all.len() < self.items.len() {
            self.items = all.to_vec();
            return vec![];
        }
        let fresh: Vec<String> = all[self.items.len()..].to_vec();
        fresh.iter().flat_map(|n| self.receive_item(n, now_ms, pos)).collect()
    }

    /// Mark quests the server says are already checked (reconnect).
    pub fn mark_checked(&mut self, ids: &[i64], now_ms: i64) {
        for id in ids {
            if self.assignments.iter().any(|a| a.location_id == *id) && self.done.insert(*id) {
                self.stats.quest_days.insert(now_ms / DAY_MS);
            }
        }
    }

    /// Re-place unfinished quests (Shuffle trap or the player's reroll). Finished quests never change.
    pub fn reroll(&mut self, ids: &[i64], realms: &[(Realm, Atlas)], seed: u64, catalog: &Catalog) -> Result<usize, String> {
        let todo: Vec<i64> = ids.iter().copied().filter(|i| !self.done.contains(i)).collect();
        let zones = zone_ctx(&self.slot, &self.zone_realms, realms)?;
        let params = AssignParams {
            home: self.home,
            minutes_per_tier: f64::from(self.slot.minutes_per_tier),
            min_distance_m: f64::from(self.slot.min_distance_m),
            seed,
            surface: self.surface,
            avoid_stairs: self.avoid_stairs,
        };
        let fresh = assign(&slots_in(&self.slot, Some(&todo)), &zones, catalog, &params);
        let n = fresh.len();
        for a in fresh {
            self.trackers.remove(&a.location_id);
            self.fog.discovered.remove(&a.location_id);
            if let Some(slot) = self.assignments.iter_mut().find(|x| x.location_id == a.location_id) {
                *slot = a;
            }
        }
        Ok(n)
    }

    /// Human-readable list of active traps (for the HUD).
    pub fn trap_labels(&self) -> Vec<String> {
        use crate::traps::Trap;
        self.traps
            .active
            .iter()
            .map(|t| match t {
                Trap::Freeze { .. } => "Frozen: reach the thaw point",
                Trap::Fog { .. } => "Fog: map hidden",
                Trap::Silence { .. } => "Silence",
                Trap::Leash { .. } => "Leash: stay near home",
                Trap::Detour { visited: false, .. } => "Detour: visit the waypoint",
                Trap::Detour { .. } => "Detour done",
                Trap::Toll { .. } => "Toll: keep moving",
                Trap::Slow { .. } => "Slow: dwell x2",
            })
            .map(String::from)
            .collect()
    }

    pub fn last_pos(&self) -> Option<Point> {
        self.last_fix.map(|f| f.point())
    }

    pub fn blocked_reason(&self) -> Option<String> {
        self.last_fix.and_then(|f| self.traps.blocks_checks(f.point()))
    }

    pub fn streak_days(&self, now_ms: i64) -> u32 {
        streak(&self.stats.quest_days, now_ms / DAY_MS)
    }

    pub fn path_for(dir: &Path, id: &str) -> PathBuf {
        let safe: String = id.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_').collect();
        dir.join("games").join(format!("{safe}.json"))
    }

    pub fn save(&self, dir: &Path) -> Result<(), String> {
        let path = Self::path_for(dir, &self.id);
        std::fs::create_dir_all(path.parent().unwrap_or(dir)).map_err(|e| e.to_string())?;
        std::fs::write(path, serde_json::to_string(self).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
    }

    pub fn load(dir: &Path, id: &str) -> Result<Game, String> {
        let s = std::fs::read_to_string(Self::path_for(dir, id)).map_err(|e| e.to_string())?;
        serde_json::from_str(&s).map_err(|e| format!("corrupt game file: {e}"))
    }

    pub fn list_ids(dir: &Path) -> Vec<(String, String)> {
        let mut out = Vec::new();
        if let Ok(rd) = std::fs::read_dir(dir.join("games")) {
            for e in rd.flatten() {
                if let Ok(s) = std::fs::read_to_string(e.path()) {
                    if let Ok(v) = serde_json::from_str::<serde_json::Value>(&s) {
                        if let (Some(id), Some(name)) = (v["id"].as_str(), v["name"].as_str()) {
                            out.push((id.to_string(), name.to_string()));
                        }
                    }
                }
            }
        }
        out.sort();
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geo::destination;
    use crate::realm::Shape;
    use crate::scan::build_atlas;
    use crate::solo::{generate, SoloOptions};

    fn home() -> Point {
        Point::new(40.0, -111.0)
    }

    fn realm(id: &str, mode: Mode) -> (Realm, Atlas) {
        let r = Realm {
            id: id.into(),
            name: format!("Realm {id}"),
            mode,
            shape: Shape::Circle { center: home(), radius_m: 6000.0 },
            spare: None,
            scanned_at_ms: None,
        };
        let mut streets = Vec::new();
        for n in -40..=40 {
            for e in -40..=40 {
                streets.push(destination(destination(home(), 0.0, f64::from(n) * 130.0), 90.0, f64::from(e) * 130.0));
            }
        }
        let a = build_atlas(id, 0, vec![], streets, &Catalog::builtin());
        (r, a)
    }

    fn reach_only(zones: &[Mode], trips: u32, goal: &str) -> SoloOptions {
        SoloOptions { zone_modes: zones.to_vec(), number_of_trips: trips, goal: goal.into(), quest_types: vec!["reach".into()], ..SoloOptions::default() }
    }

    fn game(o: &SoloOptions, backend: Backend, seed: u64) -> Game {
        let g = generate(o, seed).unwrap();
        let realms: Vec<(Realm, Atlas)> = o.zone_modes.iter().enumerate().map(|(i, m)| realm(&format!("r{i}"), *m)).collect();
        let zr = (0..o.zone_modes.len()).map(|i| format!("r{i}")).collect();
        Game::create(
            NewGame {
                id: "g1".into(),
                name: "Test".into(),
                backend,
                seed_name: "s".into(),
                slot: g.slot,
                zone_realms: zr,
                realms: &realms,
                home: home(),
                seed,
                solo_rewards: g.rewards,
                surface: SurfacePref::Any,
                avoid_stairs: false,
            },
            &Catalog::builtin(),
        )
        .unwrap()
    }

    fn fixat(p: Point, t_s: i64) -> Fix {
        Fix { lat: p.lat, lon: p.lon, t_ms: t_s * 1000, accuracy_m: 8.0 }
    }

    /// Teleport (long gaps so speed gating does not apply) to every open quest until none remain.
    fn play_all(g: &mut Game, mut t: i64) -> Vec<Event> {
        let mut all = Vec::new();
        for _ in 0..500 {
            let Some(q) = g.quest_views().into_iter().find(|q| matches!(q.state, QuestState::Open | QuestState::InProgress)) else { break };
            t += 1000;
            all.extend(g.on_fix(fixat(q.anchor.expect("reach quests have a point"), t), None));
        }
        all
    }

    #[test]
    fn a_new_solo_game_assigns_everything_and_locks_later_zones() {
        let g = game(&reach_only(&[Mode::Walk, Mode::Bike], 20, "all_trips"), Backend::Solo, 4);
        assert_eq!(g.assignments.len(), 20);
        let views = g.quest_views();
        assert!(views.iter().any(|v| v.zone == 2 && v.state == QuestState::Locked));
        assert!(views.iter().filter(|v| v.zone == 1).all(|v| v.state == QuestState::Open));
    }

    #[test]
    fn quest_views_carry_the_kind_id_so_the_ui_can_pick_an_icon() {
        let g = game(&reach_only(&[Mode::Walk], 5, "all_trips"), Backend::Solo, 4);
        for (v, a) in g.quest_views().iter().zip(&g.assignments) {
            assert!(!v.kind_id.is_empty());
            assert_eq!(v.kind_id, a.kind_id);
        }
    }

    #[test]
    fn reaching_a_quest_completes_it_once_and_pays_the_solo_reward() {
        let mut g = game(&reach_only(&[Mode::Walk], 10, "all_trips"), Backend::Solo, 4);
        let q = g.quest_views().remove(0);
        let ev = g.on_fix(fixat(q.anchor.unwrap(), 100), None);
        assert!(ev.iter().any(|e| matches!(e, Event::QuestDone { location_id, .. } if *location_id == q.location_id)));
        assert!(ev.iter().any(|e| matches!(e, Event::Reward { .. })));
        assert!(g.on_fix(fixat(q.anchor.unwrap(), 200), None).iter().all(|e| !matches!(e, Event::QuestDone { .. })), "no double completion");
        assert_eq!(g.quest_views().iter().filter(|v| v.state == QuestState::Done).count(), 1);
    }

    #[test]
    fn full_playthrough_unlocks_zones_and_reports_the_goal_exactly_once() {
        let mut g = game(&reach_only(&[Mode::Walk, Mode::Bike, Mode::Drive], 30, "all_trips"), Backend::Solo, 9);
        let ev = play_all(&mut g, 0);
        assert!(ev.iter().filter(|e| matches!(e, Event::ZoneUnlocked { .. })).count() >= 2, "zones 2 and 3 unlock along the way");
        assert_eq!(ev.iter().filter(|e| matches!(e, Event::GoalAchieved { .. })).count(), 1);
        assert_eq!(g.done.len(), 30);
        assert!(g.goal_status(0).achieved);
    }

    #[test]
    fn archipelago_backend_sends_checks_and_unlocks_from_synced_items() {
        let mut g = game(&reach_only(&[Mode::Walk, Mode::Bike], 20, "all_trips"), Backend::Archipelago, 2);
        let q = g.quest_views().into_iter().find(|v| v.zone == 1).unwrap();
        let ev = g.on_fix(fixat(q.anchor.unwrap(), 50), None);
        assert!(ev.iter().any(|e| matches!(e, Event::SendCheck { .. })) && !ev.iter().any(|e| matches!(e, Event::Reward { .. })));
        let ev = g.sync_items(&["Progressive Zone Key".into(), "Bike".into()], 60, None);
        assert!(ev.contains(&Event::ZoneUnlocked { zone: 2 }));
        let again = g.sync_items(&["Progressive Zone Key".into(), "Bike".into()], 70, None);
        assert!(again.is_empty(), "already-seen items trigger nothing");
        assert!(g.quest_views().iter().any(|v| v.zone == 2 && v.state == QuestState::Open));
    }

    #[test]
    fn freeze_trap_blocks_progress_until_you_reach_the_thaw_point() {
        let mut g = game(&reach_only(&[Mode::Walk], 10, "all_trips"), Backend::Archipelago, 4);
        let ev = g.receive_item("Freeze Trap", 0, Some(home()));
        assert!(ev.iter().any(|e| matches!(e, Event::Trap { .. })));
        let thaw = g.traps.thaw_point().unwrap();
        let q = g.quest_views().remove(0);
        let ev = g.on_fix(fixat(q.anchor.unwrap(), 100), None);
        assert!(!ev.iter().any(|e| matches!(e, Event::QuestDone { .. })), "frozen: the check must not count");
        g.on_fix(fixat(thaw, 200), None);
        assert!(g.traps.thaw_point().is_none());
        // long gap: a teleport this far in under two minutes would (correctly) look like driving to a walking quest
        let ev = g.on_fix(fixat(q.anchor.unwrap(), 1000), None);
        assert!(ev.iter().any(|e| matches!(e, Event::QuestDone { .. })));
    }

    #[test]
    fn fog_hides_far_quests_and_discovery_reveals_them() {
        let mut o = reach_only(&[Mode::Walk], 20, "all_trips");
        o.fog_of_war = true;
        let mut g = game(&o, Backend::Solo, 6);
        assert!(g.quest_views().iter().filter(|v| v.state == QuestState::Hidden).count() >= 10, "most quests start hidden");
        let target = g.quest_views().into_iter().find(|v| v.state == QuestState::Hidden).unwrap();
        let ev = g.on_fix(fixat(destination(target.anchor.unwrap(), 0.0, 120.0), 10), None);
        assert!(ev.iter().any(|e| matches!(e, Event::Discovered { location_id } if *location_id == target.location_id)));
        assert!(matches!(g.quest_views().into_iter().find(|v| v.location_id == target.location_id).unwrap().state, QuestState::Open));
    }

    #[test]
    fn walking_quests_do_not_count_while_moving_at_vehicle_speed() {
        let mut g = game(&reach_only(&[Mode::Walk], 10, "all_trips"), Backend::Solo, 4);
        let q = g.quest_views().remove(0);
        let start = destination(q.anchor.unwrap(), 0.0, 1000.0);
        g.on_fix(fixat(start, 0), None);
        let ev = g.on_fix(fixat(q.anchor.unwrap(), 10), None); // 1 km in 10 s = 360 km/h
        assert!(!ev.iter().any(|e| matches!(e, Event::QuestDone { .. })));
    }

    #[test]
    fn realm_mode_must_match_zone_mode_and_reroll_keeps_finished_quests() {
        let o = reach_only(&[Mode::Walk, Mode::Bike], 12, "all_trips");
        let g1 = generate(&o, 1).unwrap();
        let bad = vec![realm("r0", Mode::Walk), realm("r1", Mode::Walk)];
        let err = Game::create(
            NewGame {
                id: "x".into(),
                name: "x".into(),
                backend: Backend::Solo,
                seed_name: "s".into(),
                slot: g1.slot,
                zone_realms: vec!["r0".into(), "r1".into()],
                realms: &bad,
                home: home(),
                seed: 1,
                solo_rewards: g1.rewards,
                surface: SurfacePref::Any,
                avoid_stairs: false,
            },
            &Catalog::builtin(),
        );
        assert!(err.err().unwrap().contains("tagged"));

        let mut g = game(&o, Backend::Solo, 1);
        let q = g.quest_views().into_iter().find(|v| v.zone == 1).unwrap();
        g.on_fix(fixat(q.anchor.unwrap(), 1), None);
        let realms = vec![realm("r0", Mode::Walk), realm("r1", Mode::Bike)];
        let before: Vec<String> = g.assignments.iter().map(|a| format!("{:?}", a.target)).collect();
        let n = g.reroll(&g.assignments.iter().map(|a| a.location_id).collect::<Vec<_>>(), &realms, 99, &Catalog::builtin()).unwrap();
        assert_eq!(n, 11, "the finished quest is not rerolled");
        let done_id = q.location_id;
        assert_eq!(
            format!("{:?}", g.assignments.iter().find(|a| a.location_id == done_id).unwrap().target),
            before[g.assignments.iter().position(|a| a.location_id == done_id).unwrap()]
        );
    }

    #[test]
    fn save_load_round_trip_and_corrupt_files() {
        let dir = std::env::temp_dir().join(format!("apgo-game-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut g = game(&reach_only(&[Mode::Walk], 10, "all_trips"), Backend::Solo, 4);
        let q = g.quest_views().remove(0);
        g.on_fix(fixat(q.anchor.unwrap(), 5), None);
        g.save(&dir).unwrap();
        let back = Game::load(&dir, "g1").unwrap();
        assert_eq!(back.done, g.done);
        assert_eq!(back.items, g.items);
        assert_eq!(Game::list_ids(&dir), vec![("g1".to_string(), "Test".to_string())]);
        std::fs::write(Game::path_for(&dir, "g1"), "{broken").unwrap();
        assert!(Game::load(&dir, "g1").is_err());
        assert!(Game::load(&dir, "missing").is_err());
    }

    #[test]
    fn poor_accuracy_fixes_are_ignored() {
        let mut g = game(&reach_only(&[Mode::Walk], 10, "all_trips"), Backend::Solo, 4);
        let q = g.quest_views().remove(0);
        let mut f = fixat(q.anchor.unwrap(), 5);
        f.accuracy_m = 300.0;
        assert!(g.on_fix(f, None).is_empty());
    }
}
