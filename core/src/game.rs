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
use crate::goal::{evaluate, evaluate_each, GoalCtx, GoalStatus};
use crate::journal::JournalEvent;
use crate::realm::Realm;
use crate::scan::Atlas;
use crate::slot::GoalSpec;
use crate::slot::SlotData;
use crate::traps::Traps;
use crate::verify::{implied_speed_kmh, Fix, Status, Tracker, MAX_ACCURACY_M, MAX_OUTLIER_STREAK, MAX_PLAUSIBLE_KMH};

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

/// What became of the most recent fix.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
enum Verdict {
    #[default]
    Used,
    /// Rejected: the phone's own error radius was too big.
    Blurry(f64),
    /// Dropped: it implied an impossible jump from the last good position.
    Jump,
}

/// A quest the player is close to, with the reason it does or does not count at this moment.
#[derive(Debug, Clone, PartialEq)]
pub struct NearMiss {
    pub location_id: i64,
    pub name: String,
    pub distance_m: f64,
    pub reason: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Stats {
    pub distance_m: f64,
    pub quest_days: BTreeSet<i64>,
}

/// Distance from home for time-away chains when nothing better is known (old saves).
pub const DEFAULT_AWAY_M: f64 = 1000.0;
const AUTO_AWAY_SHARE: f64 = 0.4;
const AUTO_AWAY_MIN_M: f64 = 300.0;
const AUTO_AWAY_MAX_M: f64 = 3000.0;
const CUSTOM_AWAY_MIN_M: f64 = 100.0;
const CUSTOM_AWAY_MAX_M: f64 = 20_000.0;

/// What the player chose in New Game for time-away quests.
#[derive(Debug, Clone, PartialEq)]
pub struct AwayOptions {
    /// Count time away only inside a zone's area (false: anywhere).
    pub zone_only: bool,
    /// A fixed distance in metres; `None` picks one from the realm's size.
    pub custom_m: Option<f64>,
}

impl Default for AwayOptions {
    fn default() -> Self {
        AwayOptions { zone_only: true, custom_m: None }
    }
}

impl AwayOptions {
    /// The away distance for a zone whose realm reaches `farthest_m` from home.
    pub fn resolve(&self, farthest_m: f64) -> f64 {
        match self.custom_m {
            Some(m) => m.clamp(CUSTOM_AWAY_MIN_M, CUSTOM_AWAY_MAX_M),
            None => (farthest_m * AUTO_AWAY_SHARE).clamp(AUTO_AWAY_MIN_M, AUTO_AWAY_MAX_M),
        }
    }
}

/// The saved form of [`AwayOptions`]: the distance is resolved per zone when the game is created.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AwayConfig {
    pub zone_only: bool,
    pub distance_m: BTreeMap<u32, f64>,
}

impl Default for AwayConfig {
    fn default() -> Self {
        AwayConfig { zone_only: true, distance_m: BTreeMap::new() }
    }
}

impl AwayConfig {
    pub fn distance_for(&self, zone: u32) -> f64 {
        self.distance_m.get(&zone).copied().unwrap_or(DEFAULT_AWAY_M)
    }
}

/// Saved progress of the progressive quests.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Counters {
    /// Chain id -> steps credited or minutes away (map squares come from `Fog::cells`).
    pub progress: BTreeMap<String, f64>,
    /// The last step-counter reading seen this session (reset when a game is opened).
    pub steps_last: Option<i64>,
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
    #[serde(default)]
    pub away: AwayConfig,
    #[serde(default)]
    pub counters: Counters,
    #[serde(skip)]
    trackers: BTreeMap<i64, Tracker>,
    #[serde(skip)]
    last_fix: Option<Fix>,
    #[serde(skip)]
    outlier_streak: u32,
    #[serde(skip)]
    last_verdict: Verdict,
    #[serde(skip)]
    last_speed: Option<f64>,
    #[serde(skip)]
    odo_anchor: Option<Point>,
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
    pub away: AwayOptions,
}

/// How close the player must get for the quest's checkpoint (None for quests without one).
fn reach_radius(t: &Target) -> Option<f64> {
    match t {
        Target::Point { r, .. } | Target::Dwell { r, .. } | Target::DwellArea { r, .. } | Target::Courier { r, .. } | Target::RoundTrip { r, .. } => Some(*r),
        Target::Line { corridor_m, .. } => Some(*corridor_m),
        _ => None,
    }
}

/// Distance only counts after moving at least this far from the last counted point.
const ODOMETER_MIN_STEP_M: f64 = 6.0;

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
        let away =
            AwayConfig { zone_only: n.away.zone_only, distance_m: zones.iter().map(|z| (z.zone, n.away.resolve(z.realm.shape.farthest_m(n.home)))).collect() };
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
            away,
            counters: Counters::default(),
            trackers: BTreeMap::new(),
            last_fix: None,
            outlier_streak: 0,
            last_verdict: Verdict::Used,
            last_speed: None,
            odo_anchor: None,
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

    fn goal_ctx(&self, now_ms: i64) -> GoalCtx<'_> {
        GoalCtx {
            slot: &self.slot,
            assignments: &self.assignments,
            done: &self.done,
            items: &self.items,
            distance_m: self.stats.distance_m,
            cells_discovered: self.fog.cells.len(),
            streak_days: streak(&self.stats.quest_days, now_ms / DAY_MS),
        }
    }

    /// Each goal of the game with its own progress.
    pub fn goal_statuses(&self, now_ms: i64) -> Vec<(GoalSpec, GoalStatus)> {
        evaluate_each(&self.goal_ctx(now_ms))
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
            self.last_verdict = Verdict::Blurry(fix.accuracy_m);
            return ev;
        }
        // A fix that implies an impossible jump (a network or cell fix far off) is dropped, so it can neither complete a quest nor become
        // the reference for the next speed check. After a few in a row the newest is believed: you really did relocate.
        if self.last_fix.as_ref().and_then(|l| implied_speed_kmh(l, &fix)).is_some_and(|kmh| kmh > MAX_PLAUSIBLE_KMH)
            && self.outlier_streak < MAX_OUTLIER_STREAK - 1
        {
            self.outlier_streak += 1;
            self.last_verdict = Verdict::Jump;
            return ev;
        }
        self.outlier_streak = 0;
        self.last_verdict = Verdict::Used;
        let pos = fix.point();
        let speed = self.last_fix.as_ref().and_then(|l| implied_speed_kmh(l, &fix));
        self.last_speed = speed;
        // Distance walked counts only once you are clearly away from where the last counted point was (GPS wobble while standing is not movement).
        if self.last_fix.is_some_and(|l| fix.t_ms - l.t_ms > 300_000) {
            self.odo_anchor = None;
        }
        let moved = match self.odo_anchor {
            Some(a) => {
                let d = distance_m(a, pos);
                if d >= fix.accuracy_m.max(ODOMETER_MIN_STEP_M) {
                    self.odo_anchor = Some(pos);
                    d
                } else {
                    0.0
                }
            }
            None => {
                self.odo_anchor = Some(pos);
                0.0
            }
        };
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

    /// Journal entries for events, with the reason attached: how a quest was completed, where a reward came from, what an item does.
    pub fn journal_events(&self, ev: &[Event], t_ms: i64, at: Option<(f64, f64)>) -> Vec<JournalEvent> {
        let quest = |id: &i64| self.assignments.iter().find(|a| a.location_id == *id);
        ev.iter()
            .map(|e| {
                let mut j = JournalEvent::from_game_event(e, t_ms, at);
                match e {
                    Event::QuestDone { location_id, name } => {
                        if let Some(a) = quest(location_id) {
                            j.detail = format!("{name} ({}): {}", a.place, a.target.goal_text());
                        }
                    }
                    Event::Reward { location_id, item } => {
                        let from = quest(location_id).map_or("a quest", |a| a.quest_name.as_str());
                        j.detail = format!("{item} (reward for {from}): {}", crate::items::blurb(item));
                    }
                    Event::SendCheck { location_id } => {
                        if let Some(a) = quest(location_id) {
                            j.detail = format!("{} sent to the server", a.quest_name);
                        }
                    }
                    Event::Trap { item, message } => j.detail = format!("{item}: {message} ({})", crate::items::blurb(item)),
                    _ => {}
                }
                j
            })
            .collect()
    }

    /// For every open quest within `radius_m` of the fix: the distance and why it would or would not count right now.
    pub fn explain_near(&self, fix: &Fix, radius_m: f64) -> Vec<NearMiss> {
        let blocked = self.traps.blocks_checks(fix.point());
        self.assignments
            .iter()
            .filter(|a| !self.done.contains(&a.location_id))
            .filter_map(|a| {
                let at = crate::fog::anchor(&a.target)?;
                let distance_m = distance_m(fix.point(), at);
                if distance_m > radius_m {
                    return None;
                }
                let reach = reach_radius(&a.target);
                let reason = match self.last_verdict {
                    Verdict::Blurry(acc) => format!("GPS accuracy {acc:.0} m (needs {MAX_ACCURACY_M:.0} m or better)"),
                    Verdict::Jump => "ignored as a GPS jump".to_string(),
                    Verdict::Used if !self.zone_unlocked(a.zone) => format!("zone {} is still locked", a.zone),
                    Verdict::Used if self.fog_on() && !self.fog.discovered.contains(&a.location_id) => "not discovered yet (fog of war)".to_string(),
                    Verdict::Used if blocked.is_some() => blocked.clone().unwrap_or_default(),
                    Verdict::Used if self.last_speed.is_some_and(|s| !speed_ok(a.mode, s)) => {
                        format!("moving too fast for {:?} ({:.0} km/h)", a.mode, self.last_speed.unwrap_or(0.0))
                    }
                    Verdict::Used => match reach {
                        Some(r) if distance_m <= r => format!("in range ({distance_m:.0} m, needs {r:.0} m): counting"),
                        Some(r) => format!("{distance_m:.0} m away, needs {r:.0} m"),
                        None => format!("{distance_m:.0} m away"),
                    },
                };
                Some(NearMiss { location_id: a.location_id, name: a.quest_name.clone(), distance_m, reason })
            })
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

    /// Take a game out of the list but keep its save file in `games-archive/` (a played game's data is evidence, never thrown away).
    pub fn archive(dir: &Path, id: &str) -> Result<(), String> {
        let from = Self::path_for(dir, id);
        if !from.exists() {
            return Ok(());
        }
        let to = dir.join("games-archive").join(from.file_name().ok_or("bad game id")?);
        std::fs::create_dir_all(dir.join("games-archive")).map_err(|e| e.to_string())?;
        std::fs::rename(&from, &to).map_err(|e| e.to_string())
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

    fn realm(id: &str, _mode: Mode) -> (Realm, Atlas) {
        let r = Realm {
            id: id.into(),
            name: format!("Realm {id}"),
            icon: None,
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
                away: AwayOptions::default(),
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
    fn any_realm_can_serve_zones_of_any_mode_because_travel_is_the_games_choice() {
        let o = reach_only(&[Mode::Walk, Mode::Bike], 12, "all_trips");
        let g1 = generate(&o, 1).unwrap();
        let realms = vec![realm("r0", Mode::Walk)];
        let created = Game::create(
            NewGame {
                id: "x".into(),
                name: "x".into(),
                backend: Backend::Solo,
                seed_name: "s".into(),
                slot: g1.slot,
                zone_realms: vec!["r0".into(), "r0".into()], // one realm, used for a walking zone and a biking zone
                realms: &realms,
                home: home(),
                seed: 1,
                solo_rewards: g1.rewards,
                surface: SurfacePref::Any,
                avoid_stairs: false,
                away: AwayOptions::default(),
            },
            &Catalog::builtin(),
        );
        assert!(created.is_ok());
    }

    #[test]
    fn reroll_keeps_finished_quests() {
        let o = reach_only(&[Mode::Walk, Mode::Bike], 12, "all_trips");
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
    fn archiving_moves_the_save_out_of_the_list_but_keeps_it() {
        let dir = std::env::temp_dir().join(format!("apgo-archive-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let g = game(&reach_only(&[Mode::Walk], 10, "all_trips"), Backend::Solo, 4);
        g.save(&dir).unwrap();
        Game::archive(&dir, "g1").unwrap();
        assert!(Game::list_ids(&dir).is_empty());
        assert!(dir.join("games-archive").join("g1.json").exists());
        assert!(Game::archive(&dir, "g1").is_ok(), "archiving twice (or a missing game) is not an error");
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn start_near_a_quest() -> (Game, i64, Point) {
        let mut g = game(&reach_only(&[Mode::Walk], 10, "all_trips"), Backend::Solo, 4);
        let q = g.quest_views().remove(0);
        let target = q.anchor.unwrap();
        let p0 = destination(target, 0.0, 200.0);
        g.on_fix(Fix { accuracy_m: 5.0, ..fixat(p0, 1000) }, None);
        (g, q.location_id, p0)
    }

    #[test]
    fn a_far_off_network_style_fix_is_dropped_even_on_the_target() {
        let (mut g, id, p0) = start_near_a_quest();
        let target = g.assignments.iter().find(|a| a.location_id == id).and_then(|a| crate::fog::anchor(&a.target)).unwrap();
        let ev = g.on_fix(Fix { accuracy_m: 5.0, ..fixat(target, 1003) }, None);
        assert!(ev.is_empty() && !g.done.contains(&id), "a 200 m jump in 3 s is a bad fix and must not complete the quest");
        assert_eq!(g.last_pos(), Some(p0), "the last good position is kept");
    }

    #[test]
    fn walking_to_the_target_still_completes_after_an_outlier() {
        let (mut g, id, _) = start_near_a_quest();
        let target = g.assignments.iter().find(|a| a.location_id == id).and_then(|a| crate::fog::anchor(&a.target)).unwrap();
        g.on_fix(Fix { accuracy_m: 5.0, ..fixat(target, 1003) }, None); // dropped
        let ev = g.on_fix(Fix { accuracy_m: 5.0, ..fixat(target, 1100) }, None); // 200 m in 100 s: a brisk walk
        assert!(ev.iter().any(|e| matches!(e, Event::QuestDone { location_id, .. } if *location_id == id)));
    }

    #[test]
    fn several_far_fixes_in_a_row_are_believed() {
        let (mut g, _, p0) = start_near_a_quest();
        let far = destination(p0, 90.0, 5000.0);
        for (i, t) in [1003, 1006, 1009].into_iter().enumerate() {
            g.on_fix(Fix { accuracy_m: 5.0, ..fixat(far, t) }, None);
            assert_eq!(g.last_pos() == Some(far), i == 2, "believed only on the third in a row (step {i})");
        }
    }

    #[test]
    fn fixes_worse_than_35_m_are_ignored() {
        let (mut g, id, _) = start_near_a_quest();
        let target = g.assignments.iter().find(|a| a.location_id == id).and_then(|a| crate::fog::anchor(&a.target)).unwrap();
        let ev = g.on_fix(Fix { accuracy_m: 40.0, ..fixat(target, 2000) }, None);
        assert!(ev.is_empty() && !g.done.contains(&id));
    }

    #[test]
    fn standing_still_with_gps_jitter_adds_no_distance() {
        let (mut g, _, p0) = start_near_a_quest();
        for i in 0..60 {
            let wobble = destination(p0, (i * 97 % 360) as f64, 3.0 + (i % 3) as f64);
            g.on_fix(Fix { accuracy_m: 5.0, ..fixat(wobble, 1005 + i * 5) }, None);
        }
        assert!(g.stats.distance_m < 12.0, "jitter counted as {} m", g.stats.distance_m);
    }

    #[test]
    fn a_long_gap_is_not_walked_distance() {
        let (mut g, _, p0) = start_near_a_quest();
        g.on_fix(Fix { accuracy_m: 5.0, ..fixat(destination(p0, 90.0, 20_000.0), 1000 + 3600) }, None);
        assert!(g.stats.distance_m < 1.0, "an hour later 20 km away is a relocation, counted {}", g.stats.distance_m);
    }

    #[test]
    fn walking_adds_about_the_distance_walked() {
        let (mut g, _, p0) = start_near_a_quest();
        for i in 1..=40 {
            g.on_fix(Fix { accuracy_m: 5.0, ..fixat(destination(p0, 90.0, 7.0 * i as f64), 1000 + i * 5) }, None);
        }
        let d = g.stats.distance_m;
        assert!((d - 280.0).abs() < 28.0, "walked 280 m, counted {d}");
    }

    #[test]
    fn a_completed_quest_is_logged_with_how_it_was_done_and_its_reward_with_where_it_came_from() {
        let mut g = game(&reach_only(&[Mode::Walk], 10, "all_trips"), Backend::Solo, 4);
        let q = g.quest_views().remove(0);
        let ev = g.on_fix(fixat(q.anchor.unwrap(), 100), None);
        let at = Some((q.anchor.unwrap().lat, q.anchor.unwrap().lon));
        let log = g.journal_events(&ev, 100_000, at);
        let done = log.iter().find(|e| e.kind == "quest_done").expect("a quest_done entry");
        assert!(done.detail.contains(&q.name) && done.detail.contains("Get within 40 m"), "{}", done.detail);
        let reward = log.iter().find(|e| e.kind == "reward").expect("a reward entry");
        assert!(reward.detail.contains(&format!("reward for {}", q.name)), "{}", reward.detail);
        assert!(reward.detail.len() > q.name.len() + 20, "carries an explanation of the item: {}", reward.detail);
        assert!(log.iter().all(|e| e.t_ms == 100_000 && e.at == at));
    }

    #[test]
    fn near_a_quest_the_reason_it_does_or_does_not_count_is_explained() {
        let (mut g, id, p0) = start_near_a_quest();
        let target = g.assignments.iter().find(|a| a.location_id == id).and_then(|a| crate::fog::anchor(&a.target)).unwrap();
        let near = |g: &Game, p: Point, acc: f64| g.explain_near(&Fix { accuracy_m: acc, ..fixat(p, 1000) }, 100.0).into_iter().find(|n| n.location_id == id);
        assert!(near(&g, p0, 5.0).is_none(), "200 m away is not near");
        let nm = near(&g, destination(target, 0.0, 70.0), 5.0).expect("70 m away is near");
        assert!((nm.distance_m - 70.0).abs() < 2.0 && nm.reason.contains("needs 40"), "{nm:?}");
        // The state the game is in decides the reason.
        let jump = Fix { accuracy_m: 5.0, ..fixat(target, 1003) };
        g.on_fix(jump, None); // dropped as a jump
        assert!(g.explain_near(&jump, 100.0).iter().any(|n| n.location_id == id && n.reason.contains("jump")));
        let blurry = Fix { accuracy_m: 60.0, ..fixat(target, 2000) };
        g.on_fix(blurry, None);
        assert!(g.explain_near(&blurry, 100.0).iter().any(|n| n.location_id == id && n.reason.contains("accuracy 60")));
    }

    #[test]
    fn poor_accuracy_fixes_are_ignored() {
        let mut g = game(&reach_only(&[Mode::Walk], 10, "all_trips"), Backend::Solo, 4);
        let q = g.quest_views().remove(0);
        let mut f = fixat(q.anchor.unwrap(), 5);
        f.accuracy_m = 300.0;
        assert!(g.on_fix(f, None).is_empty());
    }

    #[test]
    fn automatic_away_distance_is_40_percent_of_the_realm_reach_within_limits() {
        let auto = AwayOptions { zone_only: true, custom_m: None };
        assert_eq!(auto.resolve(1000.0), 400.0);
        assert_eq!(auto.resolve(100.0), 300.0, "never below 300 m");
        assert_eq!(auto.resolve(50_000.0), 3000.0, "never above 3 km");
    }

    #[test]
    fn a_custom_away_distance_is_used_but_kept_sane() {
        let custom = |m| AwayOptions { zone_only: false, custom_m: Some(m) };
        assert_eq!(custom(1500.0).resolve(1000.0), 1500.0);
        assert_eq!(custom(5.0).resolve(1000.0), 100.0);
        assert_eq!(custom(1e9).resolve(1000.0), 20_000.0);
    }

    #[test]
    fn a_new_game_gets_a_distance_for_every_zone_and_empty_counters() {
        let g = game(&reach_only(&[Mode::Walk, Mode::Bike], 20, "all_trips"), Backend::Solo, 4);
        assert!(g.away.zone_only);
        assert_eq!(g.away.distance_m.len(), g.slot.zones.len());
        assert!(g.away.distance_m.values().all(|d| (300.0..=3000.0).contains(d)), "{:?}", g.away);
        assert!(g.counters.progress.is_empty() && g.counters.steps_last.is_none());
    }

    #[test]
    fn a_saved_game_from_before_chains_loads_with_defaults() {
        let g = game(&reach_only(&[Mode::Walk], 10, "all_trips"), Backend::Solo, 4);
        let mut v = serde_json::to_value(&g).unwrap();
        v.as_object_mut().unwrap().remove("away");
        v.as_object_mut().unwrap().remove("counters");
        let back: Game = serde_json::from_value(v).unwrap();
        assert_eq!(back.away, AwayConfig::default());
        assert!(back.counters.progress.is_empty());
        assert_eq!(back.away.distance_for(1), DEFAULT_AWAY_M);
    }
}
