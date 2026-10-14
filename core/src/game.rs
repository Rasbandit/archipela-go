//! A running game: assignments, progress, rewards, fog, traps, goal. Backend is either Solo (local reward table)
//! or Archipelago (checks go to the server, items come back). Everything else is identical.

use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use rand::rngs::StdRng;
use rand::SeedableRng;
use serde::{Deserialize, Serialize};

use crate::assign::{
    assign, forage_pool, place_of, replace_unpicked, zone_paths, AssignParams, Assignment, SlotIn, SurfacePref, Target, ZoneCtx, ITEM_SPACING_M,
};
use crate::catalog::{Catalog, Mode};
use crate::chain::{self, is_chain_target, Chain, ChainUnit};
use crate::fog::{anchor, reveal_radius, Fog};
use crate::forage::{Forage, BAG_SIZE, LIVE_PER_NEED, RESPAWN_MS};
use crate::geo::{densify, distance_m, Point};
use crate::goal::{evaluate, evaluate_each, GoalCtx, GoalStatus};
use crate::journal::JournalEvent;
use crate::loc::calib::StepCal;
use crate::loc::graph::StreetGraph;
use crate::loc::{imm::mode_cap_mps, DisplayPosition, Estimate, HeadingIn, Locator, Odometer, RawFix, Source, Verdict as LocVerdict, MAX_UNCERTAINTY_M};
use crate::near_path::PathIndex;
use crate::num::{count_f64, count_u32, i64_to_f64, round_i64, to_f32};
use crate::realm::Realm;
use crate::recap::Recap;
use crate::scan::Atlas;
use crate::slot::{check_schema, SlotData};
use crate::solo::badge_icon;
use crate::traps::Traps;
use crate::units::{distance, distance_rounded, speed_kmh, Round, UnitSystem};
use crate::verify::{collect_progress, Collected, Fix, Status, Tracker, MAX_ACCURACY_M};

const DAY_MS: i64 = 86_400_000;

/// Where checks are decided and rewards come from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Backend {
    /// Played alone: the app holds the reward table.
    Solo,
    /// Played in an Archipelago multiworld: checks go to the server and items come back.
    Archipelago,
}

/// Where a quest stands for the player.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuestState {
    /// Its zone is not unlocked yet.
    Locked,
    /// Hidden by fog until the player gets near.
    Hidden,
    /// Available to do.
    Open,
    /// Being tracked right now.
    InProgress,
    /// Completed.
    Done,
}

/// A quest as the UI shows it.
#[derive(Debug, Clone)]
pub struct QuestView {
    /// Archipelago location id of the check.
    pub location_id: i64,
    /// Zone number the quest is in.
    pub zone: u32,
    /// Display name of the quest.
    pub name: String,
    /// Name of the place the quest uses.
    pub place: String,
    /// Atlas id of the quest's place, if it has a real one (see `Game::quest_feature`).
    pub feature_id: Option<String>,
    /// What the quest's place is ("Bench"), if it is on a real place.
    pub place_type: Option<String>,
    /// Whether the map knows more about the place (see `poi_details::about`).
    pub has_about: bool,
    /// Quest family.
    pub family: String,
    /// The quest kind's id ("`bench_warmer`"), for choosing an icon.
    pub kind_id: String,
    /// Difficulty band: easy, medium or hard.
    pub difficulty: String,
    /// Effort tier, starting at 1.
    pub tier: u8,
    /// Expected effort in minutes.
    pub effort_min: f64,
    /// How the player travels there.
    pub mode: Mode,
    /// The quest's badges: items that must all be held before it can be played.
    pub needs: Vec<String>,
    /// The badges in `needs` the player does not hold yet (a quest with any is Locked).
    pub missing: Vec<String>,
    /// Each badge as the card shows it.
    pub badges: Vec<BadgeView>,
    /// The badge its map pin shows: the first still missing, else the first it needs; `None` for an open quest.
    pub pin_badge: Option<BadgeView>,
    /// What it still needs, for the card ("Needs Running Shoes"); `None` when nothing is missing.
    pub badge_note: Option<String>,
    /// Where the quest stands for the player.
    pub state: QuestState,
    /// Progress from 0 to 1.
    pub progress: f32,
    /// Where the quest is on the map, if it has a place.
    pub anchor: Option<Point>,
    /// What the player has to do.
    pub target: Target,
    /// True when a street quest stands in for a family the realm could not offer.
    pub fallback: bool,
    /// Whether this is the realm's boss quest.
    pub boss: bool,
    /// Short description of the quest kind.
    pub blurb: String,
    /// Solo only: what the quest gave you (shown after it is done).
    pub reward: Option<String>,
    /// The chain this quest is a milestone of, if any.
    pub chain_id: Option<String>,
    /// A forager quest's items picked, carried and banked so far (`None` for other quests, or before anything was picked).
    pub collected: Option<Collected>,
    /// A courier quest's pick-up time while the package is carried (for the time left to deliver).
    pub picked_at_ms: Option<i64>,
    /// A forager quest's items still on the map (empty for other quests and once it is done).
    pub live_items: Vec<Point>,
}

/// One milestone of a chain, as the UI shows it.
#[derive(Debug, Clone, PartialEq)]
pub struct MarkView {
    /// Counter value at which the milestone is reached.
    pub at: f64,
    /// Archipelago location id of the check it unlocks.
    pub location_id: i64,
    /// Whether the counter has reached it.
    pub reached: bool,
    /// Solo only: what the milestone gave you, once reached.
    pub reward: Option<String>,
}

/// A progressive quest chain as the UI shows it.
#[derive(Debug, Clone)]
pub struct ChainView {
    /// Chain id.
    pub id: String,
    /// Zone number the chain is in.
    pub zone: u32,
    /// Catalog id of the quest kind.
    pub kind_id: String,
    /// Display name.
    pub name: String,
    /// Quest family.
    pub family: String,
    /// What the counter counts.
    pub unit: ChainUnit,
    /// Current counter value.
    pub counter: f64,
    /// Counter value at the last milestone.
    pub total: f64,
    /// Short rule text, e.g. how time away is counted.
    pub rule: String,
    /// The milestones in order.
    pub marks: Vec<MarkView>,
    /// The badges every quest of the chain needs.
    pub needs: Vec<String>,
    /// The badges in `needs` the player does not hold yet (the chain counts nothing while any are missing).
    pub missing: Vec<String>,
    /// Each badge as the chain row shows it.
    pub badges: Vec<BadgeView>,
    /// What it still needs ("Needs Running Shoes"); `None` when nothing is missing.
    pub badge_note: Option<String>,
}

/// One badge (a tool a quest or chain needs) as the UI shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BadgeView {
    /// The tool item.
    pub item: String,
    /// Which icon the app draws for it (see `solo::badge_icon`).
    pub icon: String,
    /// Whether the player does not hold it yet.
    pub missing: bool,
}

/// The badges still missing, joined for a sentence ("Running Shoes and Gloves"); `None` when nothing is missing.
fn missing_text(missing: &[String]) -> Option<String> {
    (!missing.is_empty()).then(|| missing.join(" and "))
}

/// Something that happened that the UI or the server needs to hear about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// A quest was completed.
    QuestDone {
        /// Archipelago location id of the check.
        location_id: i64,
        /// Display name of the quest.
        name: String,
    },
    /// A check must be sent to the server.
    SendCheck {
        /// Archipelago location id of the check.
        location_id: i64,
    },
    /// A reward was received for a check.
    Reward {
        /// Archipelago location id of the check.
        location_id: i64,
        /// Name of the item.
        item: String,
    },
    /// A zone became available.
    ZoneUnlocked {
        /// Zone number.
        zone: u32,
    },
    /// A trap started.
    Trap {
        /// Name of the trap item.
        item: String,
        /// What the trap does, for the player.
        message: String,
        /// Whether to alert the player (a loud notification and buzz): not while an earlier Silence Trap is active.
        alert: bool,
    },
    /// The player asked to reshuffle the quests.
    ShuffleRequested,
    /// A hidden quest was revealed.
    Discovered {
        /// Archipelago location id of the check.
        location_id: i64,
    },
    /// The win condition was met.
    GoalAchieved {
        /// Text describing the goal.
        label: String,
    },
    /// A general message.
    Info {
        /// The message.
        text: String,
    },
}

impl Event {
    /// Whether this event is lasting progress the game must be saved for at once (a kill would lose it), including a trap ending
    /// (told as Info). The others wait for the periodic save: a reveal comes back on the next walk past, a trap is re-sent by the
    /// server from the saved item index.
    #[must_use]
    pub fn saves_at_once(&self) -> bool {
        !matches!(self, Self::Discovered { .. } | Self::Trap { .. })
    }
}

/// A quest the player is close to, with the reason it does or does not count at this moment.
#[derive(Debug, Clone, PartialEq)]
pub struct NearMiss {
    /// Archipelago location id of the quest.
    pub location_id: i64,
    /// Display name of the quest.
    pub name: String,
    /// How far away the player is, in metres.
    pub distance_m: f64,
    /// Why it did not count.
    pub reason: String,
}

/// Running totals for the open game.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Stats {
    /// Total distance travelled, in metres.
    pub distance_m: f64,
    /// Days (since the Unix epoch) on which a quest was completed.
    pub quest_days: BTreeSet<i64>,
}

/// Within this of the home pin a fix counts as being home: ends time away for players with no saved home Wi-Fi (with one, presence
/// stops counting at home anyway).
const HOME_RADIUS_M: f64 = 100.0;

/// The most one unbroken stretch of time away can credit: a missed event (app killed, a wake-up the phone slept through) never
/// credits more. A real outing keeps counting: a wake-up is scheduled by then at the latest.
const AWAY_MAX_STRETCH_MS: i64 = 2 * 3_600_000;
/// How long a leave mark stays good: past the 45 s debounce that confirms leaving, with room for late step batches. An older one
/// is from a Wi-Fi blip, and crediting from it would count steps walked indoors.
const LEAVE_MARK_MAX_MS: i64 = 120_000;

fn yes() -> bool {
    true
}

/// Saved progress of the progressive quests.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Counters {
    /// Chain id -> steps credited, minutes away or new map squares seen (each only while the chain's zone is unlocked).
    pub progress: BTreeMap<String, f64>,
    /// The last step-counter reading seen this session (reset when a game is opened).
    pub steps_last: Option<i64>,
    /// Whether map squares are counted in `progress`. False in saves from before that, whose squares were the whole game's `Fog::cells`.
    #[serde(default)]
    pub cells_counted: bool,
    /// While away from home: up to when time away has been credited (the next event credits from here). `None` at home.
    #[serde(default)]
    pub away_mark: Option<i64>,
    /// Whether the last good fix was within reach of home. Unlike the last fix it survives counting going off and on (a car ride),
    /// so counting coming back on at home does not start time away for a player with no saved home Wi-Fi.
    #[serde(default)]
    pub last_at_home: Option<bool>,
}

/// The Archipelago server a game connects to: its address (`host:port`, with or without a scheme) and the slot it plays as.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApServer {
    /// Server address; the port changes when an archipelago.gg room moves.
    pub address: String,
    /// Slot name.
    pub slot: String,
}

/// A walk away from home, open from leaving until arriving home (car rides in between do not close it). Snapshots, not a log:
/// what changed is worked out on arrival (`arrive_home`). Saved, so a restart mid-walk keeps it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Outing {
    /// When the player left home, Unix ms.
    pub start_ms: i64,
    /// Raw steps walked so far (not slowed by the backpack).
    pub steps: i64,
    /// `fog.cells.len()` on leaving.
    pub cells_at_start: usize,
    /// Quests done on leaving.
    pub done_at_start: BTreeSet<i64>,
    /// `items.len()` on leaving.
    pub items_at_start: usize,
    /// Zones unlocked on leaving.
    pub zones_at_start: BTreeSet<u32>,
    /// Forager items banked on leaving (all foragers). A fix inside the home radius banks before presence says "at home".
    #[serde(default)]
    pub banked_at_start: u32,
}

/// A game in progress: its quests, progress, rewards, fog, traps and goal. Saved as JSON.
#[derive(Serialize, Deserialize)]
#[allow(clippy::struct_excessive_bools)] // persisted state flags, each independent
pub struct Game {
    /// Unique id of the game.
    pub id: String,
    /// Display name.
    pub name: String,
    /// Where checks are decided.
    pub backend: Backend,
    /// Name of the multiworld seed.
    pub seed_name: String,
    /// The `slot_data` the game was created from.
    pub slot: SlotData,
    /// Realm id for each zone, in zone order.
    pub zone_realms: Vec<String>,
    /// Home point that distances are measured from.
    pub home: Point,
    /// The quest assigned to every location.
    pub assignments: Vec<Assignment>,
    /// Location ids of completed quests.
    pub done: BTreeSet<i64>,
    /// Names of items received so far.
    pub items: Vec<String>,
    /// Solo only: the item found at each location.
    pub solo_rewards: BTreeMap<i64, String>,
    /// What the player has uncovered on the map.
    pub fog: Fog,
    /// Active traps.
    pub traps: Traps,
    /// Running totals.
    pub stats: Stats,
    /// Whether the win has been reported to the server.
    pub goal_reported: bool,
    /// Archipelago: where the game connects (saved at its start, so the app reconnects after a restart). `None` for solo games and
    /// games from before servers were saved.
    #[serde(default)]
    pub ap_server: Option<ApServer>,
    /// A thinned sample of street points, the fallback for placing trap targets when no street index is attached.
    pub trap_pool: Vec<Point>,
    /// Every street and path of the game's zones, indexed to place trap targets; attached when the game is made or opened, never saved.
    #[serde(skip)]
    street_index: Option<PathIndex>,
    /// Generation of the streets last set by [`Self::set_streets`] (0: none yet), so a slower, older background build never replaces a
    /// newer one.
    #[serde(skip)]
    streets_gen: u64,
    /// Seed for random choices, so shuffles can be reproduced.
    pub seed: u64,
    /// How much rough going the player accepts.
    #[serde(default)]
    pub surface: SurfacePref,
    /// Whether quests with stairs were dropped.
    #[serde(default)]
    pub avoid_stairs: bool,
    /// Whether quests may be placed in parking lots. Saves from before the switch load as allowed: they could already use them.
    #[serde(default = "yes")]
    pub allow_parking: bool,
    /// Saved progress of progressive quests.
    #[serde(default)]
    pub counters: Counters,
    /// Forager quests: location id -> items picked, carried and banked. Kept across restarts and Shuffle traps.
    #[serde(default)]
    pub collected: BTreeMap<i64, Collected>,
    /// Forager quests: location id -> the items still on the map and when they top up (#142). Seeded from the quest's placed points
    /// (less the picked ones) the first time it is needed, so old saves load.
    #[serde(default)]
    pub forage: BTreeMap<i64, Forage>,
    /// How many items the shared backpack holds before steps slow down.
    #[serde(default = "default_bag_size")]
    pub bag_size: u32,
    /// The walk under way, if the player is away from home.
    #[serde(default)]
    pub outing: Option<Outing>,
    /// The recap of the last walk, until the player dismisses it.
    #[serde(default)]
    pub recap: Option<Recap>,
    /// Forager quests: location id -> the street points new items may appear on, from the last streets build (not saved: the next
    /// build fills it again; until then top-ups wait).
    #[serde(skip)]
    pub forage_pools: BTreeMap<i64, Vec<Point>>,
    /// Progress of each started quest (a courier pickup, a dwell's best stretch, coverage); missing in old saves. Loaded
    /// trackers are detached until `reattach_trackers`, and an entry that cannot be read is dropped, never the whole save.
    #[serde(default, deserialize_with = "lenient_trackers")]
    trackers: BTreeMap<i64, Tracker>,
    #[serde(skip)]
    last_fix: Option<Fix>,
    /// The last accepted position, kept across counting pauses (unlike `last_fix`): where traps are placed (adversarial re-review N5).
    #[serde(skip)]
    last_accepted: Option<Point>,
    /// The first step reading (total, ms) while counting is off: the phone starts the counter when home Wi-Fi or the car drops,
    /// before leaving is confirmed, so the steps walked meanwhile are credited once it is ([`LEAVE_MARK_MAX_MS`]).
    #[serde(skip)]
    leave_mark: Option<(i64, i64)>,
    /// Where the player was last seen: the last accepted or bridged (from steps) estimate. New forager items keep clear of it.
    #[serde(skip)]
    last_seen: Option<Point>,
    /// The location filter: raw fixes in, estimates out. Never saved (a restart starts fresh).
    #[serde(skip)]
    locator: Locator,
    /// Distance travelled, from accepted estimates.
    #[serde(skip)]
    odometer: Odometer,
    /// The newest estimate the filter made.
    #[serde(skip)]
    last_est: Option<Estimate>,
    /// When each zone last unlocked in this session, so time away before it is not credited (a loaded game counts its open zones from the first fix).
    #[serde(skip)]
    unlocked_at: BTreeMap<u32, i64>,
    // An estimate was not accepted (blurry, gated, unusable or bridged) since the last accepted one: the player may have left a
    // dwell unseen, so a scheduled tick must not finish one until an accepted estimate says where they are.
    #[serde(skip)]
    unseen_since_good: bool,
    #[serde(skip)]
    last_block: Option<String>,
    /// The units the game's text is written in: the player's setting, set by the app, never saved.
    #[serde(skip)]
    units: UnitSystem,
    #[serde(skip_serializing, default = "yes")]
    counting: bool,
}

/// What it takes to create a game.
pub struct NewGame<'a> {
    /// Unique id of the game.
    pub id: String,
    /// Display name.
    pub name: String,
    /// Where checks are decided.
    pub backend: Backend,
    /// Name of the multiworld seed.
    pub seed_name: String,
    /// The `slot_data` to play.
    pub slot: SlotData,
    /// Realm id for each zone, in zone order.
    pub zone_realms: Vec<String>,
    /// The scanned realms, with their atlases.
    pub realms: &'a [(Realm, Atlas)],
    /// Home point that distances are measured from.
    pub home: Point,
    /// Seed for random choices.
    pub seed: u64,
    /// Solo only: the item found at each location.
    pub solo_rewards: BTreeMap<i64, String>,
    /// How much rough going the player accepts.
    pub surface: SurfacePref,
    /// Whether to drop quests with stairs.
    pub avoid_stairs: bool,
    /// Whether quests may be placed in parking lots.
    pub allow_parking: bool,
}

/// How close the player must get for the quest's checkpoint (None for quests without one).
fn reach_radius(t: &Target) -> Option<f64> {
    match t {
        Target::Point { r, .. }
        | Target::Dwell { r, .. }
        | Target::DwellArea { r, .. }
        | Target::Courier { r, .. }
        | Target::RoundTrip { r, .. }
        | Target::Collect { r, .. } => Some(*r),
        Target::Line { corridor_m, .. } => Some(*corridor_m),
        _ => None,
    }
}

/// Whether quests of `mode` count at `kmh`: the mode's speed cap (`loc::imm::mode_cap_mps`, one table for the filter and the game).
fn speed_ok(mode: Mode, kmh: f64) -> bool {
    kmh / 3.6 <= mode_cap_mps(mode)
}

/// The lower bound of an estimate's speed (two sigmas under it), km/h, so GPS noise does not block a slow walker.
fn speed_floor_kmh(est: &Estimate) -> f64 {
    (est.speed_mps - 2.0 * est.speed_sigma_mps).max(0.0) * 3.6
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

/// What [`Game::build_streets`] makes: the trap-target street index and the location filter's street graph.
pub type Streets = (PathIndex, Option<Arc<StreetGraph>>);

/// Every street point and the streets between them, of all `atlases` (the game's zones), as one index.
fn street_index(atlases: &[&Atlas]) -> PathIndex {
    let mut points = Vec::new();
    let mut links = Vec::new();
    for a in atlases {
        points.extend(a.streets.iter().chain(&a.streets_rough).copied());
        links.extend(a.street_links(false).into_iter().chain(a.street_links(true)));
    }
    PathIndex::with_segments(&points, &links)
}

/// About how many street points of each realm the trap pool keeps.
const TRAP_POOL_PER_ZONE: usize = 600;

/// Street and path points from every zone's realm (each realm once, thinned), so a trap's point to reach is on a street wherever the player is.
fn trap_pool(zones: &[ZoneCtx<'_>]) -> Vec<Point> {
    let mut seen = BTreeSet::new();
    let mut pool = Vec::new();
    for z in zones.iter().filter(|z| seen.insert(z.realm.id.as_str())) {
        let all: Vec<Point> = z.atlas.streets.iter().chain(&z.atlas.streets_rough).copied().collect();
        pool.extend(all.iter().step_by((all.len() / TRAP_POOL_PER_ZONE).max(1)).copied());
    }
    pool
}

fn default_bag_size() -> u32 {
    BAG_SIZE
}

/// A forager's live items from its placed points, less those `c` has picked (their ids are the placed indexes).
fn seed_forage(pts: &[Point], c: Option<&Collected>) -> Forage {
    let mut f = Forage::seed(pts);
    if let Some(c) = c {
        f.items.retain(|i| u16::try_from(i.id).map_or(true, |i| !c.picked.contains(&i)));
    }
    f
}

/// A repeatable random source for one top-up of quest `id`, varied by `salt` (the time, or a Shuffle trap's seed).
fn forage_rng(id: i64, salt: u64) -> StdRng {
    StdRng::seed_from_u64(id.cast_unsigned().wrapping_mul(0x9E37_79B9) ^ salt)
}

fn zone_ctx<'a>(slot: &SlotData, zone_realms: &[String], realms: &'a [(Realm, Atlas)]) -> Result<Vec<ZoneCtx<'a>>, String> {
    let mut out = Vec::new();
    for (i, z) in slot.zones.iter().enumerate() {
        let id = zone_realms.get(i).ok_or_else(|| format!("zone {} has no realm assigned", z.id))?;
        let (realm, atlas) = realms.iter().find(|(r, _)| &r.id == id).map(|(r, a)| (r, a)).ok_or_else(|| format!("realm {id} not found"))?;
        out.push(ZoneCtx { zone: z.id, realm, atlas });
    }
    Ok(out)
}

fn slots_in(slot: &SlotData, only: Option<&[i64]>) -> Vec<SlotIn> {
    slot.all_quests()
        .into_iter()
        .filter(|q| only.is_none_or(|ids| ids.contains(&q.location_id)))
        .map(|q| SlotIn {
            location_id: q.location_id,
            zone: q.zone,
            mode: q.mode,
            family: q.family.clone(),
            tier: q.effort_tier,
            boss: q.family == "boss",
            needs: q.needs.clone(),
        })
        .collect()
}

// Each saved tracker on its own: one that a later version cannot read is dropped instead of failing the whole save.
fn lenient_trackers<'de, D: serde::Deserializer<'de>>(d: D) -> Result<BTreeMap<i64, Tracker>, D::Error> {
    let raw: BTreeMap<i64, serde_json::Value> = Deserialize::deserialize(d)?;
    Ok(raw.into_iter().filter_map(|(id, v)| serde_json::from_value(v).ok().map(|t| (id, t))).collect())
}

impl Game {
    /// Create a game: assign a quest to every slot using the scanned realms.
    ///
    /// # Errors
    /// Returns a message if a zone has no realm assigned or its realm is missing.
    pub fn create(n: NewGame<'_>, catalog: &Catalog) -> Result<Self, String> {
        let zones = zone_ctx(&n.slot, &n.zone_realms, n.realms)?;
        let params = AssignParams {
            home: n.home,
            minutes_per_tier: f64::from(n.slot.minutes_per_tier),
            min_distance_m: f64::from(n.slot.min_distance_m),
            seed: n.seed,
            surface: n.surface,
            avoid_stairs: n.avoid_stairs,
            allow_parking: n.allow_parking,
            allow_progressive: true,
        };
        let assignments = assign(&slots_in(&n.slot, None), &zones, catalog, &params);
        let pool = trap_pool(&zones);
        Ok(Self {
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
            ap_server: None,
            trap_pool: pool,
            street_index: None, // the background refresh builds it with the graph (`build_streets`), once (final review M6)
            streets_gen: 0,
            seed: n.seed,
            surface: n.surface,
            avoid_stairs: n.avoid_stairs,
            allow_parking: n.allow_parking,
            counters: Counters { cells_counted: true, ..Counters::default() },
            collected: BTreeMap::new(),
            forage: BTreeMap::new(),
            bag_size: BAG_SIZE,
            outing: None,
            recap: None,
            forage_pools: BTreeMap::new(),
            trackers: BTreeMap::new(),
            last_fix: None,
            last_accepted: None,
            leave_mark: None,
            last_seen: None,
            unlocked_at: BTreeMap::new(),
            unseen_since_good: false,
            locator: Locator::default(),
            odometer: Odometer::default(),
            last_est: None,
            last_block: None,
            units: UnitSystem::default(),
            counting: true,
        }
        .with_default_travel_mode())
    }

    fn count(&self, item: &str) -> u32 {
        count_u32(self.items.iter().filter(|i| *i == item).count())
    }

    /// Whether the player holds enough zone keys to enter `zone`.
    #[must_use]
    pub fn zone_unlocked(&self, zone: u32) -> bool {
        let keys = self.count("Progressive Zone Key");
        self.slot.zone(zone).is_some_and(|z| keys >= z.zone_keys_needed)
    }

    /// The badges in `needs` the player does not hold yet.
    #[must_use]
    pub fn missing_needs(&self, needs: &[String]) -> Vec<String> {
        needs.iter().filter(|n| !self.items.contains(n)).cloned().collect()
    }

    /// Each of `needs` as the UI shows it: its icon and whether it is still missing.
    #[must_use]
    pub fn badge_views(&self, needs: &[String]) -> Vec<BadgeView> {
        needs.iter().map(|n| BadgeView { item: n.clone(), icon: badge_icon(n).into(), missing: !self.items.contains(n) }).collect()
    }

    /// What `needs` still asks for, as the card says it ("Needs Running Shoes"); `None` when every badge is held.
    #[must_use]
    pub fn badge_note(&self, needs: &[String]) -> Option<String> {
        missing_text(&self.missing_needs(needs)).map(|t| format!("Needs {t}"))
    }

    /// Whether a quest in `zone` that needs `needs` can be played: its zone is open and every badge is held (`needs` within the items).
    #[must_use]
    pub fn quest_unlocked(&self, zone: u32, needs: &[String]) -> bool {
        self.zone_unlocked(zone) && needs.iter().all(|n| self.items.contains(n))
    }

    fn unlocked_set(&self) -> BTreeSet<u32> {
        self.slot.zones.iter().map(|z| z.id).filter(|z| self.zone_unlocked(*z)).collect()
    }

    fn goal_ctx(&self) -> GoalCtx<'_> {
        GoalCtx { slot: &self.slot, done: &self.done, items: &self.items }
    }

    /// Each goal of the game with its own progress.
    #[must_use]
    pub fn goal_statuses(&self) -> Vec<(String, GoalStatus)> {
        evaluate_each(&self.goal_ctx())
    }

    /// How far along the win condition is.
    #[must_use]
    pub fn goal_status(&self) -> GoalStatus {
        evaluate(&self.goal_ctx())
    }

    fn fog_on(&self) -> bool {
        self.slot.fog_of_war
    }

    /// Every quest as the UI shows it.
    #[must_use]
    pub fn quest_views(&self, now_ms: i64) -> Vec<QuestView> {
        let chains = self.chains();
        self.assignments
            .iter()
            .map(|a| {
                let done = self.done.contains(&a.location_id);
                let hidden = (self.fog_on() && !self.fog.discovered.contains(&a.location_id)) || (self.traps.fog_active() && !done);
                let member = self.member_progress(&chains, a.location_id, now_ms);
                let progress = member.as_ref().map_or_else(
                    || {
                        self.trackers.get(&a.location_id).map_or_else(
                            || self.saved_progress(a),
                            |t| match t.status() {
                                Status::Active(p) => p,
                                Status::Done => 1.0,
                                Status::Idle => 0.0,
                            },
                        )
                    },
                    |(_, p)| *p,
                );
                let state = if done {
                    QuestState::Done
                } else if !self.quest_unlocked(a.zone, &a.needs) {
                    QuestState::Locked
                } else if hidden {
                    QuestState::Hidden
                } else if progress > 0.0 {
                    QuestState::InProgress
                } else {
                    QuestState::Open
                };
                let difficulty = self.slot.all_quests().iter().find(|q| q.location_id == a.location_id).map(|q| q.difficulty.clone()).unwrap_or_default();
                let badges = self.badge_views(&a.needs);
                QuestView {
                    location_id: a.location_id,
                    zone: a.zone,
                    name: a.quest_name.clone(),
                    place: a.place.clone(),
                    feature_id: a.feature_id.clone(),
                    place_type: a.place_type.clone(),
                    has_about: a.has_about,
                    family: a.family.clone(),
                    kind_id: a.kind_id.clone(),
                    difficulty,
                    tier: a.tier,
                    effort_min: a.effort_min,
                    mode: a.mode,
                    needs: a.needs.clone(),
                    missing: self.missing_needs(&a.needs),
                    pin_badge: badges.iter().find(|b| b.missing).or(badges.first()).cloned(),
                    badges,
                    badge_note: self.badge_note(&a.needs),
                    state,
                    progress,
                    anchor: anchor(&a.target),
                    // As the tracker counts it (a Slow trap doubles dwells), so the card shows the time actually needed.
                    target: self.adjusted(&a.target),
                    fallback: a.fallback,
                    boss: a.boss,
                    blurb: a.blurb.clone(),
                    reward: if done { self.solo_rewards.get(&a.location_id).cloned() } else { None },
                    chain_id: member.map(|(id, _)| id),
                    collected: self.collected.get(&a.location_id).cloned(),
                    picked_at_ms: self.trackers.get(&a.location_id).and_then(Tracker::picked_at),
                    live_items: if done { Vec::new() } else { self.live_items(a) },
                }
            })
            .collect()
    }

    /// Keep each quest's place (`feature_id`, `place_type`, `has_about`) in step with `atlases` (the game's zones as scanned): link
    /// quests saved before quests kept their place (see [`place_of`]) and refresh what a rescan changed. Returns how many quests
    /// changed, so the caller saves the game only then.
    pub fn relink_places(&mut self, atlases: &[&Atlas], catalog: &Catalog) -> usize {
        let by_id: Vec<std::collections::HashMap<&str, &crate::scan::Feature>> =
            atlases.iter().map(|x| x.features.iter().map(|f| (f.id.as_str(), f)).collect()).collect();
        let mut changed = 0;
        for a in &mut self.assignments {
            let Some(i) = self.slot.zones.iter().position(|z| z.id == a.zone) else { continue };
            let Some(k) = self.zone_realms.get(i).and_then(|r| atlases.iter().position(|x| &x.realm_id == r)) else { continue };
            let Some(id) = a.feature_id.clone().or_else(|| place_of(a, atlases[k], catalog)) else { continue };
            // A rescan can drop the place or its facts: then the quest keeps its type but has nothing more to show.
            let facts = by_id[k].get(id.as_str()).map_or((a.place_type.clone(), false), |f| crate::poi_details::place_facts(f));
            // Count only real changes, so a settled game is not saved on every build.
            if a.feature_id.as_deref() != Some(id.as_str()) || (a.place_type.clone(), a.has_about) != facts {
                (a.place_type, a.has_about) = facts;
                a.feature_id = Some(id);
                changed += 1;
            }
        }
        changed
    }

    /// The realm and atlas feature id behind quest `location_id`, for its place details; `None` for a quest with no real place.
    #[must_use]
    pub fn quest_feature(&self, location_id: i64) -> Option<(&str, &str)> {
        let a = self.assignments.iter().find(|a| a.location_id == location_id)?;
        let feature = a.feature_id.as_deref()?;
        let i = self.slot.zones.iter().position(|z| z.id == a.zone)?;
        Some((self.zone_realms.get(i)?.as_str(), feature))
    }

    /// Every progressive chain as the UI shows it.
    #[must_use]
    pub fn chain_views(&self, now_ms: i64) -> Vec<ChainView> {
        self.chains()
            .into_iter()
            .map(|c| {
                let counter = self.counter_at(&c, now_ms);
                let family = self
                    .assignments
                    .iter()
                    .find(|a| c.marks.first().is_some_and(|m| m.location_id == a.location_id))
                    .map(|a| a.family.clone())
                    .unwrap_or_default();
                let marks = c
                    .marks
                    .iter()
                    .map(|m| {
                        let reached = self.done.contains(&m.location_id);
                        MarkView {
                            at: m.at,
                            location_id: m.location_id,
                            reached,
                            reward: if reached { self.solo_rewards.get(&m.location_id).cloned() } else { None },
                        }
                    })
                    .collect();
                ChainView {
                    rule: c.rule_text(),
                    total: c.total(),
                    missing: self.missing_needs(&c.needs),
                    badges: self.badge_views(&c.needs),
                    badge_note: self.badge_note(&c.needs),
                    needs: c.needs,
                    id: c.id,
                    zone: c.zone,
                    kind_id: c.kind_id,
                    name: c.name,
                    family,
                    unit: c.unit,
                    counter,
                    marks,
                }
            })
            .collect()
    }

    /// Progress of a chain member toward its own mark (0..1): done = 1, the next unreached mark = how far through its stretch, later ones 0.
    fn member_progress(&self, chains: &[Chain], location_id: i64, now_ms: i64) -> Option<(String, f32)> {
        let c = chains.iter().find(|c| c.position_of(location_id).is_some())?;
        let i = c.position_of(location_id)? - 1;
        let counter = self.counter_at(c, now_ms);
        let at = c.marks[i].at;
        let prev = if i == 0 { 0.0 } else { c.marks[i - 1].at };
        let p = if counter >= at {
            1.0
        } else if counter <= prev {
            0.0
        } else {
            (counter - prev) / (at - prev)
        };
        Some((c.id.clone(), to_f32(p)))
    }

    /// Progress kept in the save for a quest that has no tracker yet this session (a forager after a restart).
    fn saved_progress(&self, a: &Assignment) -> f32 {
        match (&a.target, self.collected.get(&a.location_id)) {
            (Target::Collect { need, .. }, Some(c)) => collect_progress(c, *need),
            _ => 0.0,
        }
    }

    /// Feeds a fix to a quest's tracker (made on first use, resuming saved forager progress) and keeps a forager's progress in the save.
    fn update_tracker(&mut self, id: i64, fix: &Fix, steps_total: Option<i64>) -> Option<Status> {
        if !self.trackers.contains_key(&id) {
            let target = self.tracker_target(&self.assignments.iter().find(|a| a.location_id == id)?.target);
            let t = match self.collected.get(&id) {
                Some(c) => Tracker::with_collected(target, self.home, c.clone()),
                None => Tracker::new(target, self.home),
            };
            self.trackers.insert(id, t);
        }
        let t = self.trackers.get_mut(&id)?;
        let status = t.update(fix, steps_total);
        if let Some(c) = t.collected().filter(|c| **c != Collected::default()) {
            self.collected.insert(id, c.clone());
        }
        Some(status)
    }

    /// What a quest's tracker checks fixes against: its target as traps adjust it. Game picks forager items (they live in `forage`), so
    /// a forager's tracker gets no points: it only banks at home and reports progress.
    fn tracker_target(&self, t: &Target) -> Target {
        match self.adjusted(t) {
            Target::Collect { need, r, theme, .. } => Target::Collect { pts: Vec::new(), need, r, theme },
            t => t,
        }
    }

    fn adjusted(&self, t: &Target) -> Target {
        let m = self.traps.dwell_multiplier();
        match t {
            Target::Dwell { p, r, minutes } => Target::Dwell { p: *p, r: *r, minutes: minutes * m },
            Target::DwellArea { poly, center, r, minutes } => Target::DwellArea { poly: poly.clone(), center: *center, r: *r, minutes: minutes * m },
            other => other.clone(),
        }
    }

    /// The progressive chains of this game.
    #[must_use]
    pub fn chains(&self) -> Vec<Chain> {
        chain::derive(&self.assignments)
    }

    /// The chains that can be played (zone open, badges held): only these count progress or complete marks.
    fn unlocked_chains(&self) -> Vec<Chain> {
        self.chains().into_iter().filter(|c| self.quest_unlocked(c.zone, &c.needs)).collect()
    }

    fn counter_of(&self, c: &Chain) -> f64 {
        self.counters.progress.get(&c.id).copied().unwrap_or(0.0)
    }

    /// Credit map squares seen for the first time to every map-square chain in an unlocked zone.
    fn credit_cells(&mut self, gained: usize) {
        if gained == 0 {
            return;
        }
        for c in self.unlocked_chains().into_iter().filter(|c| c.unit == ChainUnit::Cells) {
            *self.counters.progress.entry(c.id).or_insert(0.0) += count_f64(gained);
        }
    }

    /// Credit the steps since the last reading of the phone's cumulative step counter. The first reading of a session only sets the baseline;
    /// a reading below the last one means the phone restarted its counter. Only chains in unlocked zones are credited.
    fn credit_steps(&mut self, total: i64) {
        let gained = match self.counters.steps_last {
            None => 0,
            Some(last) if total >= last => total - last,
            Some(_) => total,
        };
        self.counters.steps_last = Some(total);
        if let Some(o) = &mut self.outing {
            o.steps += gained;
        }
        if gained == 0 {
            return;
        }
        // An overloaded backpack slows the count (#142): steps are the only thing it slows.
        let counted = i64_to_f64(gained) * self.step_factor();
        for c in self.unlocked_chains().into_iter().filter(|c| c.unit == ChainUnit::Steps) {
            *self.counters.progress.entry(c.id).or_insert(0.0) += counted;
        }
    }

    /// Load the phone's saved step calibration into the location filter (ignored for another step source; never in the game save).
    pub fn set_step_calibration(&mut self, c: StepCal) {
        self.locator.set_step_calibration(c);
    }

    /// The location filter's step calibration, for the app to save.
    #[must_use]
    pub fn step_calibration(&self) -> StepCal {
        self.locator.step_calibration()
    }

    /// A step-counter reading outside a fix (the sensor reports on its own). While a GPS gap is bridged the bridged estimate uncovers fog
    /// and map squares, nothing else.
    pub fn on_steps(&mut self, total: i64, t_ms: i64, cadence: Option<f64>) -> Vec<Event> {
        let bridged = self.locator.on_steps(total, t_ms, cadence);
        if !self.counting {
            self.note_leave_steps(total, t_ms);
            return Vec::new();
        }
        self.credit_steps(total);
        if bridged.is_none() {
            self.top_up_due(t_ms, self.last_seen); // a bridged estimate tops up below, clear of where the steps put the player
        }
        let mut ev = bridged.map(|b| self.on_estimate(&b, None)).unwrap_or_default();
        ev.extend(self.complete_reached(t_ms, self.last_pos()));
        ev
    }

    /// Write the game's text (quest goals, near-miss reasons, trap messages) in `units` from now on.
    pub fn set_units(&mut self, units: UnitSystem) {
        self.units = units;
        self.traps.set_units(units);
    }

    // Leaving home: snapshot what the recap will compare with on arrival. An open outing (a car ride ended) is kept.
    fn open_outing(&mut self, t_ms: i64) {
        if self.outing.is_none() {
            self.outing = Some(Outing {
                start_ms: t_ms,
                steps: 0,
                cells_at_start: self.fog.cells.len(),
                done_at_start: self.done.clone(),
                items_at_start: self.items.len(),
                zones_at_start: self.unlocked_set(),
                banked_at_start: self.banked_total(),
            });
        }
    }

    /// Presence rules (at home, in the car) switch counting off: nothing is checked, credited or added while it is off. Counting on
    /// means away from home: time away runs from `t_ms` (unless the last fix put the player at home) until counting goes off.
    pub fn set_counting(&mut self, on: bool, t_ms: i64) {
        if on {
            let at_home = self.counters.last_at_home == Some(true);
            if self.counters.away_mark.is_none() && !at_home {
                self.counters.away_mark = Some(t_ms);
                self.open_outing(t_ms);
            }
        } else {
            self.settle_away(t_ms);
            self.counters.away_mark = None;
        }
        if self.counting == on {
            return;
        }
        self.counting = on;
        // Whatever the player did while it was off must not be compared with what they do next. Steps resume from the leave mark
        // when it is fresh (the walk out while leaving was confirmed); else the first reading after this is the new baseline.
        self.last_fix = None;
        let mark = self.leave_mark.take();
        self.counters.steps_last = mark.filter(|&(_, at)| on && t_ms.saturating_sub(at) <= LEAVE_MARK_MAX_MS).map(|(total, _)| total);
        self.odometer.clear();
        self.locator.reset();
        if !on {
            // A dwell or away timer started before a pause must not finish on the first fix after it (progress is kept).
            self.trackers.values_mut().for_each(Tracker::pause);
        }
    }

    /// The walk out began at step `total` at `t_ms` (the home watch saw it): steps from there count once counting resumes, not the
    /// pacing indoors that may have set the mark before. Ignored while counting is on (no pause to resume from).
    pub fn set_leave_mark(&mut self, total: i64, t_ms: i64) {
        if !self.counting {
            self.leave_mark = Some((total, t_ms));
        }
    }

    // A step reading while counting is off: the first one of a fresh stretch is the leave mark; a stale one is replaced.
    fn note_leave_steps(&mut self, total: i64, t_ms: i64) {
        if self.leave_mark.is_none_or(|(_, at)| t_ms.saturating_sub(at) > LEAVE_MARK_MAX_MS) {
            self.leave_mark = Some((total, t_ms));
        }
    }

    /// Whether a trap blocks checks where the player last was.
    fn checks_blocked(&self) -> bool {
        match self.last_pos() {
            Some(p) => self.traps.blocks_checks(p).is_some(),
            None => self.traps.may_block_without_position(),
        }
    }

    /// The fix spacing the phone asks for (see [`crate::loc::Locator::set_fix_spacing_hint`]).
    pub fn set_fix_spacing_hint(&mut self, ms: Option<i64>) {
        self.locator.set_fix_spacing_hint(ms);
    }

    /// The travel mode at the player's position (the engine knows the zone shapes), for the filter's motion models and the odometer's
    /// speed cap.
    pub fn set_travel_mode(&mut self, mode: Mode) {
        self.locator.set_mode(mode);
        self.odometer.set_speed_cap(mode_cap_mps(mode));
    }

    /// The game with its travel mode set (see [`Self::refresh_travel_mode`]).
    fn with_default_travel_mode(mut self) -> Self {
        let mode = self.playable_mode();
        self.set_travel_mode(mode);
        self
    }

    /// The fastest mode among the quests that can be played (zone open, badges held), so a runner is never judged as a walker, and a
    /// walker is not judged by run rules before a run quest is open.
    fn playable_mode(&self) -> Mode {
        self.assignments.iter().filter(|a| self.quest_unlocked(a.zone, &a.needs)).map(|a| a.mode).max().unwrap_or(Mode::Walk)
    }

    /// Follow the travel mode to what the held items make playable; only items change it, so it is called when they do.
    fn refresh_travel_mode(&mut self) {
        let mode = self.playable_mode();
        if mode != self.locator.mode() {
            self.set_travel_mode(mode);
        }
    }

    /// The newest estimate the filter made (what the last fix became); `None` until a fix was usable.
    #[must_use]
    pub fn last_estimate(&self) -> Option<Estimate> {
        self.last_est
    }

    /// A compass reading from the phone.
    pub fn on_heading(&mut self, h: &HeadingIn) {
        self.locator.on_heading(h);
    }

    /// What the map shows for the player at `now_ms` (display only).
    #[must_use]
    pub fn position(&self, now_ms: i64) -> Option<DisplayPosition> {
        self.locator.display(now_ms)
    }

    /// The session's display trace (matched where confident) as runs broken where the filter reset or relocated, and when it starts;
    /// the journal has the estimates before that.
    #[must_use]
    pub fn trace_matched(&self) -> (Option<i64>, Vec<Vec<Point>>) {
        self.locator.trace_matched()
    }

    /// What changed in [`Self::trace_matched`] since `cursor`: a cursor from another game (or 0) gets everything with `reset`.
    #[must_use]
    pub fn trace_matched_since(&self, cursor: u64) -> crate::loc::matcher::TraceDelta {
        self.locator.trace_since(cursor)
    }

    /// The phone joined home Wi-Fi (presence entered "at home"), with or without a GPS fix: every forager banks what it carries, and one
    /// that reaches its need is completed. Counting is off at home, so it is not checked here; a trap that blocks checks blocks this as it
    /// blocks banking on a fix. Calling it again banks nothing new.
    pub fn bank_at_home(&mut self, t_ms: i64) -> Vec<Event> {
        if self.traps.blocks_checks(self.home).is_some() {
            return Vec::new();
        }
        let mut reached = Vec::new();
        for a in &self.assignments {
            let Target::Collect { need, .. } = &a.target else { continue };
            if self.done.contains(&a.location_id) || !self.quest_unlocked(a.zone, &a.needs) {
                continue;
            }
            let Some(c) = self.collected.get_mut(&a.location_id) else { continue };
            if c.carried == 0 {
                continue;
            }
            c.bank();
            if c.banked >= *need {
                reached.push(a.location_id);
            }
            self.trackers.remove(&a.location_id); // rebuilt from `collected` on the next fix
        }
        let home = self.home;
        reached.into_iter().flat_map(|id| self.complete(id, t_ms, Some(home))).collect()
    }

    /// The player arrived home: bank what is carried (see `bank_at_home`) and close the walk. Returns the bank's events and the
    /// walk's recap when it is worth showing (route not set: the engine adds it from the journal). Safe to call again.
    pub fn arrive_home(&mut self, t_ms: i64) -> (Vec<Event>, Option<Recap>) {
        let ev = self.bank_at_home(t_ms);
        let recap = self.outing.take().map(|o| self.recap_of(&o, t_ms)).filter(Recap::worth_showing);
        (ev, recap)
    }

    // Forager items banked so far, finished foragers included.
    fn banked_total(&self) -> u32 {
        self.collected.values().map(|c| c.banked).sum()
    }

    // What changed since the outing's snapshots.
    fn recap_of(&self, o: &Outing, t_ms: i64) -> Recap {
        let quests_done: Vec<String> = self
            .assignments
            .iter()
            .filter(|a| self.done.contains(&a.location_id) && !o.done_at_start.contains(&a.location_id))
            .map(|a| a.quest_name.clone())
            .collect();
        let checks_sent = if self.backend == Backend::Archipelago { u32::try_from(quests_done.len()).unwrap_or(u32::MAX) } else { 0 };
        Recap {
            start_ms: o.start_ms,
            end_ms: t_ms,
            steps: o.steps,
            new_cells: u32::try_from(self.fog.cells.len().saturating_sub(o.cells_at_start)).unwrap_or(u32::MAX),
            quests_done,
            checks_sent,
            items: self.items.get(o.items_at_start..).unwrap_or_default().to_vec(),
            zones_unlocked: self.unlocked_set().difference(&o.zones_at_start).copied().collect(),
            banked: self.banked_total().saturating_sub(o.banked_at_start),
            ..Recap::default()
        }
    }

    /// Credit time away up to `t_ms` (from the mark, to every time-away chain of an unlocked zone, not while a trap blocks
    /// checks) and move the mark there. Called on events only: a fix, counting going off, a scheduled tick.
    fn settle_away(&mut self, t_ms: i64) {
        let Some(mark) = self.counters.away_mark else { return };
        if t_ms <= mark {
            return;
        }
        if !self.checks_blocked() {
            for c in self.unlocked_chains().into_iter().filter(|c| c.unit == ChainUnit::Minutes) {
                let from = self.unlocked_at.get(&c.zone).map_or(mark, |u| mark.max(*u));
                if t_ms > from {
                    *self.counters.progress.entry(c.id).or_insert(0.0) += i64_to_f64((t_ms - from).min(AWAY_MAX_STRETCH_MS)) / 60_000.0;
                }
            }
        }
        self.counters.away_mark = Some(t_ms);
    }

    /// Time away on a fix: one at home ends it (the stretch since the last event is not credited: when you got home is unknown);
    /// one elsewhere credits up to now (not while a trap blocks checks), or starts it.
    fn away_on_fix(&mut self, fix: &Fix, blocked: bool) {
        let at_home = distance_m(fix.point(), self.home) <= HOME_RADIUS_M;
        self.counters.last_at_home = Some(at_home);
        if at_home {
            self.counters.away_mark = None;
        } else if self.counters.away_mark.is_some() && !blocked {
            self.settle_away(fix.t_ms);
        } else {
            self.counters.away_mark = Some(fix.t_ms);
            self.open_outing(fix.t_ms);
        }
    }

    /// A chain's counter as of `now_ms`: what is credited, plus the time away since the mark (worked out when asked, nothing ticks).
    fn counter_at(&self, c: &Chain, now_ms: i64) -> f64 {
        let credited = self.counter_of(c);
        let running = self.counters.away_mark.filter(|_| c.unit == ChainUnit::Minutes && self.quest_unlocked(c.zone, &c.needs) && !self.checks_blocked());
        running.map_or(credited, |mark| {
            let from = self.unlocked_at.get(&c.zone).map_or(mark, |u| mark.max(*u));
            credited + i64_to_f64((now_ms - from).clamp(0, AWAY_MAX_STRETCH_MS)) / 60_000.0
        })
    }

    /// Whether time away is running right now (its live value moves with the clock): a screen showing it may redraw now and then.
    #[must_use]
    pub fn away_running(&self) -> bool {
        self.counters.away_mark.is_some() && !self.checks_blocked()
    }

    /// When something next falls due if nothing changes (a time-away mark, a dwell the player stands in, a forager top-up), so the app
    /// can schedule one wake-up then; `None` when nothing is due.
    #[must_use]
    pub fn next_due_ms(&self, now_ms: i64) -> Option<i64> {
        if self.checks_blocked() {
            return None; // a trap holds every check; the fix or item that ends it reschedules
        }
        let away = self.counters.away_mark.and_then(|mark| {
            let next_mark = self
                .unlocked_chains()
                .into_iter()
                .filter(|c| c.unit == ChainUnit::Minutes)
                .filter_map(|c| {
                    let live = self.counter_at(&c, now_ms);
                    let next = c.marks.iter().filter(|m| !self.done.contains(&m.location_id) && m.at > live).map(|m| m.at).reduce(f64::min)?;
                    Some(now_ms + round_i64(((next - live) * 60_000.0).ceil()))
                })
                .min()?;
            // Wake by the stretch cap at the latest, so a long outing keeps counting.
            Some(next_mark.min(mark + AWAY_MAX_STRETCH_MS))
        });
        // A dwell the player is standing in finishes then too, with no more fixes needed (not after a dropped fix: they may have left).
        let dwell = if self.unseen_since_good { None } else { self.trackers.values().filter_map(Tracker::due_ms).min() };
        // A top-up only counts when it can run (the streets build gave its pool): else the wake-up would find nothing to do, again and again.
        let respawn = self
            .forage
            .iter()
            .filter(|(id, _)| !self.done.contains(id) && self.forage_pools.get(id).is_some_and(|p| !p.is_empty()))
            .filter_map(|(_, f)| f.next_spawn_ms)
            .min();
        away.into_iter().chain(dwell).chain(respawn).min()
    }

    /// A scheduled wake-up (see [`Self::next_due_ms`]): settle time away, finish dwells still running, complete what fell due.
    pub fn tick(&mut self, t_ms: i64) -> Vec<Event> {
        self.top_up_due(t_ms, self.last_seen);
        self.settle_away(t_ms);
        let mut ev = Vec::new();
        if self.counting && !self.checks_blocked() && !self.unseen_since_good {
            let finished: Vec<i64> = self.trackers.iter_mut().filter_map(|(id, t)| (t.tick(t_ms) == Status::Done).then_some(*id)).collect();
            for id in finished {
                ev.extend(self.complete(id, t_ms, self.last_pos()));
            }
        }
        ev.extend(self.complete_reached(t_ms, self.last_pos()));
        ev
    }

    /// Complete every chain member whose mark the counter has passed (in unlocked zones, and not while a trap blocks checks).
    fn complete_reached(&mut self, t_ms: i64, pos: Option<Point>) -> Vec<Event> {
        let blocked = match pos {
            Some(p) => self.traps.blocks_checks(p).is_some(),
            None => self.traps.may_block_without_position(),
        };
        if blocked {
            return Vec::new();
        }
        let mut ev = Vec::new();
        for c in self.unlocked_chains() {
            let counter = self.counter_at(&c, t_ms);
            for id in c.reached(counter) {
                if !self.done.contains(&id) {
                    ev.extend(self.complete(id, t_ms, pos));
                }
            }
        }
        ev
    }

    /// Feed a raw GPS fix (and the cumulative step counter if the phone has one). The filter turns it into an estimate, which is all that
    /// quests, fog, chains and the odometer ever see.
    pub fn on_fix(&mut self, raw: &RawFix, steps_total: Option<i64>) -> Vec<Event> {
        self.on_fix_with_zone(raw, steps_total, |_| {})
    }

    /// [`Self::on_fix`] that hands `place` this fix's own estimate (never a bridged one), after the filter and before the checks, so the
    /// engine judges the zone (presence) from the estimate in the same pass (ruling E2).
    pub fn on_fix_with_zone(&mut self, raw: &RawFix, steps_total: Option<i64>, place: impl FnOnce(Point)) -> Vec<Event> {
        if let Some(total) = steps_total {
            self.locator.on_fix_steps(total, raw.t_ms); // a fallback only: steps come as step events at their own time (ruling FR-I3)
        }
        let est = self.locator.on_fix(raw);
        // Stored here only, for every fix and also while counting is off: the engine places the player (zone, mode) from it. An unusable
        // fix before any estimate places nobody (it may be 1000 m off, mocked or NaN).
        if est.verdict != LocVerdict::Unusable || self.last_est.is_some() {
            self.last_est = Some(est);
            if est.source != Source::Bridged {
                place(est.point());
            }
        }
        if !self.counting {
            return self.on_estimate(&est, steps_total);
        }
        if self.locator.resumed_after_bridge() {
            // No fix saw the bridged gap: it is not dwelt, as across a restart (adversarial re-review). Time away runs on: it counts
            // between events, fixes or not.
            self.trackers.values_mut().for_each(Tracker::pause);
            self.last_fix = None;
        }
        // Distance is summed here, where the locator still knows whether this fix ended a stationary hold (rulings T8-R18, R20).
        let moved = self.odometer.step_with(&est, self.locator.hold_relocation_speed());
        self.stats.distance_m += moved;
        self.check(&est, steps_total, moved)
    }

    /// Feed one position estimate. Only an accepted one checks quests, advances chains, adds distance or counts time away; a bridged one (steps
    /// in a GPS gap) only uncovers fog and map squares; a filter restart pauses dwell and away timers (progress is kept).
    pub fn on_estimate(&mut self, est: &Estimate, steps_total: Option<i64>) -> Vec<Event> {
        if !self.counting {
            if let Some(t) = steps_total {
                self.note_leave_steps(t, est.t_ms);
            }
            return Vec::new();
        }
        let moved = if est.source == Source::Bridged { 0.0 } else { self.odometer.step(est) };
        self.stats.distance_m += moved;
        self.check(est, steps_total, moved)
    }

    /// What an estimate does once counting is on and its distance (`moved`) is added: see [`Self::on_estimate`].
    fn check(&mut self, est: &Estimate, steps_total: Option<i64>, moved: f64) -> Vec<Event> {
        let mut ev = Vec::new();
        // Where the player is: this estimate when it counts (or is bridged from steps), else the last one that did.
        if est.accepted || est.source == Source::Bridged {
            self.last_seen = Some(est.point());
        }
        self.top_up_due(est.t_ms, self.last_seen);
        if let Some(total) = steps_total {
            self.credit_steps(total);
        }
        if matches!(est.verdict, LocVerdict::Reset | LocVerdict::Relocated) {
            self.trackers.values_mut().for_each(Tracker::pause);
            self.last_fix = None;
        }
        // Not accepted (or bridged from steps): where the player is now is unseen, so a scheduled tick must not finish a dwell (they may
        // have left it) until an accepted estimate says where they are.
        if est.source == Source::Bridged {
            self.unseen_since_good = true;
            self.reveal(est.point(), &mut ev);
            return ev;
        }
        if !est.accepted {
            self.unseen_since_good = true;
            return ev;
        }
        self.unseen_since_good = false;
        let fix = Fix { lat: est.lat, lon: est.lon, t_ms: est.t_ms, accuracy_m: est.uncertainty_m };
        let pos = fix.point();
        let kmh = speed_floor_kmh(est);
        self.reveal(pos, &mut ev);
        for text in self.traps.tick(fix.t_ms, pos, moved) {
            ev.push(Event::Info { text });
        }
        let blocked = self.traps.blocks_checks(pos);
        if blocked != self.last_block {
            if let Some(b) = &blocked {
                ev.push(Event::Info { text: b.clone() });
            }
            self.last_block.clone_from(&blocked);
        }
        let in_chain: BTreeSet<i64> = self.assignments.iter().filter(|a| is_chain_target(&a.target)).map(|a| a.location_id).collect();
        let mut finished = Vec::new();
        if blocked.is_none() {
            let ids: Vec<(i64, Mode, bool)> = self.assignments.iter().map(|a| (a.location_id, a.mode, self.quest_unlocked(a.zone, &a.needs))).collect();
            for (id, mode, open) in ids {
                if self.done.contains(&id) || !open || in_chain.contains(&id) {
                    continue;
                }
                if self.fog_on() && !self.fog.discovered.contains(&id) {
                    continue;
                }
                if !speed_ok(mode, kmh) {
                    continue;
                }
                self.pick_forage(id, &fix);
                if self.update_tracker(id, &fix, steps_total) == Some(Status::Done) {
                    finished.push(id);
                }
            }
        }
        for id in finished {
            ev.extend(self.complete(id, fix.t_ms, Some(pos)));
        }
        self.away_on_fix(&fix, blocked.is_some());
        ev.extend(self.complete_reached(fix.t_ms, Some(pos)));
        self.last_fix = Some(fix);
        self.last_accepted = Some(pos);
        ev
    }

    /// Uncover fog and map squares around `pos` (Cartographer chains count the new squares).
    fn reveal(&mut self, pos: Point, ev: &mut Vec<Event>) {
        let scout = self.count("Progressive Scouting Distance");
        let cells_before = self.fog.cells.len();
        for id in self.fog.update(pos, &self.assignments, reveal_radius(scout)) {
            if self.fog_on() {
                ev.push(Event::Discovered { location_id: id });
            }
        }
        self.credit_cells(self.fog.cells.len() - cells_before);
    }

    /// Items carried in the shared backpack: what every open forager carries.
    #[must_use]
    pub fn carried_total(&self) -> u32 {
        self.assignments
            .iter()
            .filter(|a| matches!(a.target, Target::Collect { .. }) && !self.done.contains(&a.location_id))
            .filter_map(|a| self.collected.get(&a.location_id))
            .map(|c| c.carried)
            .sum()
    }

    /// How much a step counts now: less while the backpack holds more than `bag_size` (see [`crate::forage::step_factor`]).
    #[must_use]
    pub fn step_factor(&self) -> f64 {
        crate::forage::step_factor(self.carried_total().saturating_sub(self.bag_size))
    }

    /// How quests of this game are placed (re-placing them, forager pools).
    fn assign_params(&self, seed: u64) -> AssignParams {
        AssignParams {
            home: self.home,
            minutes_per_tier: f64::from(self.slot.minutes_per_tier),
            min_distance_m: f64::from(self.slot.min_distance_m),
            seed,
            surface: self.surface,
            avoid_stairs: self.avoid_stairs,
            allow_parking: self.allow_parking,
            allow_progressive: false,
        }
    }

    /// A forager's items on the map, nearest home first (the first is where its card opens from the list): its `forage` entry, or
    /// (not seeded yet) its placed points less the picked ones.
    fn live_items(&self, a: &Assignment) -> Vec<Point> {
        let Target::Collect { pts, .. } = &a.target else { return Vec::new() };
        let f = self.forage.get(&a.location_id).cloned().unwrap_or_else(|| seed_forage(pts, self.collected.get(&a.location_id)));
        let mut items: Vec<Point> = f.items.iter().map(|i| i.p).collect();
        items.sort_by(|x, y| distance_m(self.home, *x).total_cmp(&distance_m(self.home, *y)));
        items
    }

    /// The forager's `forage` entry, seeded on first use; `None` for any other quest.
    fn forage_entry(&mut self, id: i64) -> Option<&mut Forage> {
        if !self.forage.contains_key(&id) {
            let a = self.assignments.iter().find(|a| a.location_id == id)?;
            let Target::Collect { pts, .. } = &a.target else { return None };
            let f = seed_forage(pts, self.collected.get(&id));
            self.forage.insert(id, f);
        }
        self.forage.get_mut(&id)
    }

    /// Picks up the forager's items within reach of `fix` into what it carries (never refused, whatever the bag holds).
    fn pick_forage(&mut self, id: i64, fix: &Fix) {
        let Some((r, placed)) = self.assignments.iter().find(|a| a.location_id == id).and_then(|a| match &a.target {
            Target::Collect { r, pts, .. } => Some((*r, pts.len())),
            _ => None,
        }) else {
            return;
        };
        if fix.accuracy_m > MAX_ACCURACY_M {
            return;
        }
        let Some(f) = self.forage_entry(id) else { return };
        let before: Vec<u32> = f.items.iter().map(|i| i.id).collect();
        let n = f.pick(fix.point(), r, fix.t_ms);
        if n == 0 {
            return;
        }
        let left: BTreeSet<u32> = f.items.iter().map(|i| i.id).collect();
        let c = self.collected.entry(id).or_default();
        c.carried += n;
        // Placed items keep their index in `picked`: the Shuffle trap moves only the unpicked ones.
        c.picked.extend(before.into_iter().filter(|i| !left.contains(i) && (*i as usize) < placed).filter_map(|i| u16::try_from(i).ok()));
        self.trackers.remove(&id); // rebuilt from `collected` on this fix
    }

    /// Tops up every open forager whose time has come, from its pool, clear of the player at `here`; one that cannot fill tries again
    /// a respawn later.
    fn top_up_due(&mut self, now_ms: i64, here: Option<Point>) {
        let pools = std::mem::take(&mut self.forage_pools);
        for (id, want, r) in self.forager_wants() {
            let Some(pool) = pools.get(&id).filter(|p| !p.is_empty()) else { continue };
            // Most events find nothing due: only then work out what to keep clear of.
            if !self.forage_entry(id).is_some_and(|f| f.due(now_ms)) {
                continue;
            }
            self.top_up_one(id, want, r, pool, here, now_ms.cast_unsigned());
            if let Some(f) = self.forage.get_mut(&id).filter(|f| f.next_spawn_ms.is_some()) {
                f.next_spawn_ms = Some(now_ms + RESPAWN_MS);
            }
        }
        self.forage_pools = pools;
    }

    /// Tops up forager `id` (pick-up radius `r`) to `want` items from `pool`: clear of other quests and foragers' items (see
    /// [`Self::forage_avoid`]) and out of reach of the player at `here`, so no item is placed and picked on one fix.
    fn top_up_one(&mut self, id: i64, want: usize, r: f64, pool: &[Point], here: Option<Point>, salt: u64) {
        let pool: Vec<Point> = pool.iter().copied().filter(|p| here.is_none_or(|h| distance_m(*p, h) > r)).collect();
        let avoid = self.forage_avoid(id, here);
        if let Some(f) = self.forage_entry(id) {
            f.top_up(want, &pool, &avoid, ITEM_SPACING_M, &mut forage_rng(id, salt));
        }
    }

    /// Where forager `id`'s new items must keep clear of: every point of other open quests (a courier's both stops, a trail's whole
    /// route), other foragers' items, and the player at `here`.
    fn forage_avoid(&self, id: i64, here: Option<Point>) -> Vec<Point> {
        let others = self.assignments.iter().filter(|a| a.location_id != id && !self.done.contains(&a.location_id));
        others
            .flat_map(|a| match &a.target {
                Target::Collect { .. } => self.live_items(a),
                Target::Courier { a, b, .. } => vec![*a, *b],
                // Points along the route close enough together that keeping clear of each keeps clear of the line between them.
                Target::Line { pts, .. } => densify(pts, ITEM_SPACING_M / 4.0),
                t => anchor(t).into_iter().collect(),
            })
            .chain(here)
            .collect()
    }

    /// Open foragers, how many items each keeps on the map, and its pick-up radius.
    fn forager_wants(&self) -> Vec<(i64, usize, f64)> {
        self.assignments
            .iter()
            .filter(|a| !self.done.contains(&a.location_id))
            .filter_map(|a| match &a.target {
                Target::Collect { need, r, .. } => Some((a.location_id, usize::try_from(need.saturating_mul(LIVE_PER_NEED)).unwrap_or(usize::MAX), *r)),
                _ => None,
            })
            .collect()
    }

    /// Where each forager's new items may appear, from the zones' current scans (the streets build calls it), and the first fill to its
    /// live size for a forager not yet filled.
    pub fn set_forage_pools(&mut self, realms: &[(Realm, Atlas)]) {
        let Ok(zones) = zone_ctx(&self.slot, &self.zone_realms, realms) else { return };
        let params = self.assign_params(0);
        let mut pools = BTreeMap::new();
        for a in &self.assignments {
            let Target::Collect { r, .. } = &a.target else { continue };
            if let Some(z) = zones.iter().find(|z| z.zone == a.zone) {
                pools.insert(a.location_id, forage_pool(z, &params, a.tier, *r, a.mode));
            }
        }
        self.forage_pools = pools;
        self.fill_unfilled();
    }

    /// Fills each open forager not filled yet (new, loaded, or moved by a Shuffle trap) to its live size from its pool, once, drawing
    /// with its `salt` (see [`Forage::salt`]).
    fn fill_unfilled(&mut self) {
        let pools = std::mem::take(&mut self.forage_pools);
        let here = self.last_seen;
        for (id, want, r) in self.forager_wants() {
            let Some(pool) = pools.get(&id) else { continue };
            let Some(salt) = self.forage_entry(id).filter(|f| !f.filled).map(|f| f.salt) else { continue };
            self.top_up_one(id, want, r, pool, here, salt);
            if let Some(f) = self.forage.get_mut(&id) {
                f.filled = true;
            }
        }
        self.forage_pools = pools;
    }

    fn complete(&mut self, id: i64, now_ms: i64, pos: Option<Point>) -> Vec<Event> {
        let mut ev = Vec::new();
        // Every way a quest finishes passes here, so this is the one place the lock is enforced (a tracker started before the server
        // took a badge back must not finish either).
        let locked = self.assignments.iter().find(|a| a.location_id == id).is_some_and(|a| !self.quest_unlocked(a.zone, &a.needs));
        if locked || !self.done.insert(id) {
            return ev;
        }
        self.trackers.remove(&id);
        if let Some(f) = self.forage.get_mut(&id) {
            f.clear(); // a done forager's items leave the map and never come back
        }
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
        ev.extend(self.check_goal());
        ev
    }

    fn check_goal(&mut self) -> Vec<Event> {
        if !self.goal_reported {
            let s = self.goal_status();
            if s.achieved {
                self.goal_reported = true;
                return vec![Event::GoalAchieved { label: s.label }];
            }
        }
        vec![]
    }

    /// The street index (trap targets) and street graph (location filter) of `atlases`, the game's zones as scanned. Slow on a big realm
    /// and needs no game: build it off the main thread and outside any lock, then hand it to [`Self::set_streets`].
    #[must_use]
    pub fn build_streets(atlases: &[&Atlas]) -> Streets {
        (street_index(atlases), StreetGraph::for_atlases(atlases).map(Arc::new))
    }

    /// Swap in streets built by [`Self::build_streets`] as `generation` (taken before building began); returns false, changing
    /// nothing, when a newer generation is already in. Cheap. Until the first one arrives the game plays without a street graph.
    pub fn set_streets(&mut self, streets: Streets, generation: u64) -> bool {
        if generation <= self.streets_gen {
            return false;
        }
        let (index, graph) = streets;
        self.street_index = Some(index);
        self.locator.set_graph(graph);
        self.streets_gen = generation;
        true
    }

    /// [`Self::build_streets`] and [`Self::set_streets`] at once, as the newest generation. Neither is saved.
    pub fn attach_streets(&mut self, atlases: &[&Atlas]) {
        let generation = self.streets_gen + 1;
        self.set_streets(Self::build_streets(atlases), generation);
    }

    /// The street graph of the game's zones (from [`Self::set_streets`]); `None` until it arrives, or without streets.
    #[must_use]
    pub fn street_graph(&self) -> Option<&Arc<StreetGraph>> {
        self.locator.graph()
    }

    /// Whether the game has a street index: set by [`Self::set_streets`] and [`Self::attach_streets`]; not after a create or a load.
    #[must_use]
    pub fn streets_attached(&self) -> bool {
        self.street_index.is_some()
    }

    /// The streets trap targets are placed on: the attached index, or the saved thin sample when none is attached.
    #[must_use]
    pub fn trap_paths(&self) -> PathIndex {
        self.street_index.clone().unwrap_or_else(|| PathIndex::new(&self.trap_pool))
    }

    /// An item arrived (solo reward or server). Applies unlocks and trap effects.
    pub fn receive_item(&mut self, name: &str, now_ms: i64, pos: Option<Point>) -> Vec<Event> {
        let ev = self.receive(name, now_ms, pos);
        self.refresh_travel_mode();
        ev
    }

    // One item, without following the travel mode (a batch does that once, after the last item).
    fn receive(&mut self, name: &str, now_ms: i64, pos: Option<Point>) -> Vec<Event> {
        // Time away up to now counts only for what was playable before this item (a zone or a badge it opens starts counting now).
        self.settle_away(now_ms);
        let before = self.unlocked_set();
        self.items.push(name.to_string());
        let mut ev = Vec::new();
        for z in self.unlocked_set().difference(&before) {
            self.unlocked_at.insert(*z, now_ms);
            ev.push(Event::ZoneUnlocked { zone: *z });
        }
        if name.ends_with("Trap") {
            let mut rng = StdRng::seed_from_u64(self.seed ^ (self.items.len() as u64).wrapping_mul(0x9E37_79B9));
            let thin;
            let paths = if let Some(i) = &self.street_index {
                i
            } else {
                thin = PathIndex::new(&self.trap_pool);
                &thin
            };
            let alert = !self.traps.silenced(now_ms); // read first: a Silence Trap's own alert still fires
            if let Some(message) = self.traps.trigger(name, now_ms, pos, self.home, paths, &mut rng) {
                ev.push(Event::Trap { item: name.to_string(), message, alert });
            }
            if name == "Shuffle Trap" {
                ev.push(Event::ShuffleRequested);
            }
        }
        ev.extend(self.check_goal());
        ev
    }

    /// Archipelago: replace the received-item list with the server's; only NEW items trigger effects.
    pub fn sync_items(&mut self, all: &[String], now_ms: i64, pos: Option<Point>) -> Vec<Event> {
        if all.len() < self.items.len() {
            self.settle_away(now_ms); // time away up to now was earned with the items held until now
            self.items = all.to_vec();
            self.refresh_travel_mode();
            return vec![];
        }
        let fresh: Vec<String> = all[self.items.len()..].to_vec();
        let ev = fresh.iter().flat_map(|n| self.receive(n, now_ms, pos)).collect();
        self.refresh_travel_mode();
        ev
    }

    /// Mark quests the server says are already checked (reconnect).
    pub fn mark_checked(&mut self, ids: &[i64], now_ms: i64) {
        for id in ids {
            if self.assignments.iter().any(|a| a.location_id == *id) && self.done.insert(*id) {
                self.stats.quest_days.insert(now_ms / DAY_MS);
            }
        }
    }

    /// Re-place unfinished quests (a Shuffle trap). Finished quests and chain members never change, and a re-placed quest never gets
    /// a progressive kind, so no chain gains, loses or shifts a mark. A forager keeps its picked items and counts and only its
    /// unpicked items move.
    ///
    /// # Errors
    /// Returns a message if a zone has no realm assigned or its realm is missing.
    pub fn reroll(&mut self, ids: &[i64], realms: &[(Realm, Atlas)], seed: u64, catalog: &Catalog) -> Result<usize, String> {
        let todo: Vec<i64> = ids
            .iter()
            .copied()
            .filter(|i| !self.done.contains(i) && !self.assignments.iter().any(|a| a.location_id == *i && is_chain_target(&a.target)))
            .collect();
        let zones = zone_ctx(&self.slot, &self.zone_realms, realms)?;
        let params = self.assign_params(seed);
        // A forager keeps its kind, counts and picked items: only what is still out there moves (and stays put if the zone has no room).
        let (foragers, todo): (Vec<i64>, Vec<i64>) =
            todo.into_iter().partition(|i| self.assignments.iter().any(|a| a.location_id == *i && matches!(a.target, Target::Collect { .. })));
        let mut rng = StdRng::seed_from_u64(seed);
        let mut moved = 0;
        for id in foragers {
            let picked = self.collected.get(&id).map(|c| c.picked.clone()).unwrap_or_default();
            let Some(a) = self.assignments.iter_mut().find(|a| a.location_id == id) else { continue };
            let Some(z) = zones.iter().find(|z| z.zone == a.zone) else { continue };
            let (_, index) = zone_paths(z, &params);
            if let Some(t) = replace_unpicked(&a.target, &picked, z, &index, &params, a.tier, a.mode, &mut rng) {
                // Seeded again from the moved points (less the picked ones), keeping when it tops up; filled below when it has a pool.
                let clock = self.forage.remove(&id).and_then(|f| f.next_spawn_ms);
                let mut f = seed_forage(
                    match &t {
                        Target::Collect { pts, .. } => pts,
                        _ => &[],
                    },
                    self.collected.get(&id),
                );
                f.next_spawn_ms = clock;
                f.salt = seed; // its refill draws other spots than the first fill, even if the streets build comes later
                self.forage.insert(id, f);
                a.target = t;
                self.trackers.remove(&id); // rebuilt from `collected` on the next fix
                moved += 1;
            }
        }
        let fresh = assign(&slots_in(&self.slot, Some(&todo)), &zones, catalog, &params);
        let n = fresh.len();
        for a in fresh {
            self.trackers.remove(&a.location_id);
            self.collected.remove(&a.location_id); // a different quest must not inherit the old counts
            self.forage.remove(&a.location_id);
            self.fog.discovered.remove(&a.location_id);
            if let Some(slot) = self.assignments.iter_mut().find(|x| x.location_id == a.location_id) {
                *slot = a;
            }
        }
        // After the other quests moved, so the refilled items keep clear of where they are now, not where they were.
        self.fill_unfilled();
        Ok(n + moved)
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
    #[must_use]
    pub fn journal_events(&self, ev: &[Event], t_ms: i64, at: Option<(f64, f64)>) -> Vec<JournalEvent> {
        let quest = |id: &i64| self.assignments.iter().find(|a| a.location_id == *id);
        let chains = self.chains();
        ev.iter()
            .map(|e| {
                let mut j = JournalEvent::from_game_event(e, t_ms, at);
                match e {
                    Event::QuestDone { location_id, name } => {
                        if let Some(c) = chains.iter().find(|c| c.position_of(*location_id).is_some()) {
                            let i = c.position_of(*location_id).unwrap_or(1);
                            j.detail = format!("{name} milestone {i} of {}: {}", c.marks.len(), c.amount_text(c.marks[i - 1].at));
                        } else if let Some(a) = quest(location_id) {
                            j.detail = format!("{name} ({}): {}", a.place, a.target.goal_text(self.units));
                        }
                    }
                    Event::Reward { location_id, item } => {
                        let from = quest(location_id).map_or("a quest", |a| a.quest_name.as_str());
                        j.detail = format!("{item} (reward for {from}): {}", crate::items::blurb(item, self.units));
                    }
                    Event::SendCheck { location_id } => {
                        if let Some(a) = quest(location_id) {
                            j.detail = format!("{} sent to the server", a.quest_name);
                        }
                    }
                    Event::Trap { item, message, .. } => j.detail = format!("{item}: {message} ({})", crate::items::blurb(item, self.units)),
                    _ => {}
                }
                j
            })
            .collect()
    }

    /// Every open-or-done quest with a point to reach: (location id, the point, how close counts). For the location bench.
    #[must_use]
    pub fn reach_targets(&self) -> Vec<(i64, Point, f64)> {
        self.assignments.iter().filter_map(|a| Some((a.location_id, anchor(&a.target)?, reach_radius(&a.target)?))).collect()
    }

    /// For every open quest within `radius_m` of the estimate: the distance and why it would or would not count right now.
    #[must_use]
    pub fn explain_near(&self, est: &Estimate, radius_m: f64) -> Vec<NearMiss> {
        let here = est.point();
        let blocked = self.traps.blocks_checks(here);
        let kmh = speed_floor_kmh(est);
        self.assignments
            .iter()
            .filter(|a| !self.done.contains(&a.location_id))
            .filter_map(|a| {
                let distance_m = match &a.target {
                    // A forager counts at any item still out there, not only its first (which may be picked).
                    Target::Collect { .. } => self.live_items(a).into_iter().map(|p| distance_m(here, p)).reduce(f64::min)?,
                    t => distance_m(here, anchor(t)?),
                };
                if distance_m > radius_m {
                    return None;
                }
                let reason = if est.source == Source::Bridged {
                    "position estimated from steps (GPS gap)".to_string()
                } else if est.uncertain() {
                    format!(
                        "GPS uncertain ({}, needs {})",
                        distance_rounded(est.uncertainty_m, self.units, Round::Up),
                        distance_rounded(MAX_UNCERTAINTY_M, self.units, Round::Down)
                    )
                } else if est.verdict == LocVerdict::Gated {
                    "ignored as a GPS jump".to_string()
                } else if est.verdict == LocVerdict::Unusable {
                    "GPS fix unusable (too coarse, stale or from a mock app)".to_string()
                } else if !self.zone_unlocked(a.zone) {
                    format!("zone {} is still locked", a.zone)
                } else if let Some(t) = missing_text(&self.missing_needs(&a.needs)) {
                    format!("needs {t}")
                } else if self.fog_on() && !self.fog.discovered.contains(&a.location_id) {
                    "not discovered yet (fog of war)".to_string()
                } else if let Some(b) = &blocked {
                    b.clone()
                } else if !speed_ok(a.mode, kmh) {
                    format!("moving too fast for {:?} ({})", a.mode, speed_kmh(kmh, self.units))
                } else {
                    match reach_radius(&a.target) {
                        Some(r) if distance_m <= r => format!(
                            "in range ({}, needs {}): counting",
                            distance_rounded(distance_m, self.units, Round::Down),
                            distance_rounded(r, self.units, Round::Down)
                        ),
                        Some(r) => {
                            format!("{} away, needs {}", distance_rounded(distance_m, self.units, Round::Up), distance_rounded(r, self.units, Round::Down))
                        }
                        None => format!("{} away", distance(distance_m, self.units)),
                    }
                };
                Some(NearMiss { location_id: a.location_id, name: a.quest_name.clone(), distance_m, reason })
            })
            .collect()
    }

    /// The last accepted position since counting was last switched on, if any (checks compare with it).
    #[must_use]
    pub fn last_pos(&self) -> Option<Point> {
        self.last_fix.map(|f| f.point())
    }

    /// The last accepted position of the session, kept across counting pauses: where an Archipelago trap is placed (adversarial
    /// re-review N5); `None` only before the first one (the trap then falls back to home).
    #[must_use]
    pub fn last_accepted_pos(&self) -> Option<Point> {
        self.last_accepted
    }

    /// Why checks are blocked right now (by a trap), if they are.
    #[must_use]
    pub fn blocked_reason(&self) -> Option<String> {
        self.last_fix.and_then(|f| self.traps.blocks_checks(f.point()))
    }

    /// Days in a row, up to today, on which a quest was completed.
    #[must_use]
    pub fn streak_days(&self, now_ms: i64) -> u32 {
        streak(&self.stats.quest_days, now_ms / DAY_MS)
    }

    /// Where the game with this id is saved under `dir`.
    #[must_use]
    pub fn path_for(dir: &Path, id: &str) -> PathBuf {
        let safe: String = id.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_').collect();
        dir.join("games").join(format!("{safe}.json"))
    }

    /// Take a game out of the list but keep its save file in `games-archive/` (a played game's data is evidence, never thrown away).
    ///
    /// # Errors
    /// Returns a message if the archive folder cannot be created or the file cannot be moved.
    pub fn archive(dir: &Path, id: &str) -> Result<(), String> {
        let from = Self::path_for(dir, id);
        if !from.exists() {
            return Ok(());
        }
        let to = dir.join("games-archive").join(from.file_name().ok_or("bad game id")?);
        std::fs::create_dir_all(dir.join("games-archive")).map_err(|e| e.to_string())?;
        std::fs::rename(&from, &to).map_err(|e| e.to_string())
    }

    // The game being played lives in this file under `dir`: written on open, removed on pause, so it survives the app being closed.
    fn playing_path(dir: &Path) -> PathBuf {
        dir.join("playing")
    }

    /// Remember that the game `id` is being played, so a restart resumes it.
    ///
    /// # Errors
    /// Returns a message if the marker cannot be written.
    pub fn mark_playing(dir: &Path, id: &str) -> Result<(), String> {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        std::fs::write(Self::playing_path(dir), id).map_err(|e| e.to_string())
    }

    /// Forget the game being played (it was paused): a restart opens on the list of saved games.
    pub fn clear_playing(dir: &Path) {
        let _ = std::fs::remove_file(Self::playing_path(dir)); // already absent is fine
    }

    /// The id of the game that was being played when the app last stopped, if its save still exists.
    #[must_use]
    pub fn playing(dir: &Path) -> Option<String> {
        let id = std::fs::read_to_string(Self::playing_path(dir)).ok()?.trim().to_string();
        (!id.is_empty() && Self::path_for(dir, &id).exists()).then_some(id)
    }

    /// Write the game to `dir`, safely: the old copy stays intact until the new one is on disk.
    ///
    /// # Errors
    /// Returns a message if the file cannot be written or the game cannot be serialised.
    pub fn save(&self, dir: &Path) -> Result<(), String> {
        let path = Self::path_for(dir, &self.id);
        std::fs::create_dir_all(path.parent().unwrap_or(dir)).map_err(|e| e.to_string())?;
        let json = serde_json::to_string(self).map_err(|e| e.to_string())?;
        // Write beside the save, flush to disk, then rename over it: a kill mid-write never corrupts the only copy.
        let tmp = path.with_extension("json.tmp");
        let mut file = std::fs::File::create(&tmp).map_err(|e| e.to_string())?;
        file.write_all(json.as_bytes()).map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;
        drop(file);
        std::fs::rename(&tmp, &path).map_err(|e| e.to_string())
    }

    /// Read the game with this id from `dir`.
    ///
    /// # Errors
    /// Returns a message if the file cannot be read or is corrupt.
    pub fn load(dir: &Path, id: &str) -> Result<Self, String> {
        /// Only the save's `slot_data` version, read first: an older save's other fields may not parse as this schema.
        #[derive(Deserialize)]
        struct Head {
            slot: SlotHead,
        }
        #[derive(Deserialize)]
        struct SlotHead {
            schema_version: u32,
        }
        let older = |e: String| format!("this game was made with an older version of the app and cannot be played ({e})");
        let s = std::fs::read_to_string(Self::path_for(dir, id)).map_err(|e| e.to_string())?;
        if let Ok(head) = serde_json::from_str::<Head>(&s) {
            check_schema(head.slot.schema_version).map_err(older)?;
        }
        let mut g: Self = serde_json::from_str(&s).map_err(|e| format!("corrupt game file: {e}"))?;
        // A goal this version no longer has could never be won: refuse the game instead of opening it.
        g.slot.check_goals().map_err(older)?;
        g.counters.steps_last = None; // steps taken while the game was closed are never credited
        g.counters.away_mark = None; // nor time away: where the player was while the app was dead is unknown (presence restarts it)
        g.reattach_trackers();
        g.normalize_counters();
        Ok(g.with_default_travel_mode())
    }

    /// Give each loaded tracker its quest's target as it stands now (a trap that made a dwell longer may be over), and pause it:
    /// nothing from while the app was closed counts (dwell time, time away, steps), but what each quest reached is kept. A tracker
    /// whose quest is gone, finished, or no longer fits its saved progress is dropped, and that quest starts over.
    fn reattach_trackers(&mut self) {
        let trackers = std::mem::take(&mut self.trackers);
        self.trackers = trackers
            .into_iter()
            .filter(|(id, _)| !self.done.contains(id))
            .filter_map(|(id, mut t)| {
                let target = self.tracker_target(&self.assignments.iter().find(|a| a.location_id == id)?.target);
                t.pause();
                t.reattach(target, self.home).then_some((id, t))
            })
            .collect();
    }

    /// An old save has finished chain members but no counters: start each counter at its highest finished mark so nothing is lost or earned twice.
    /// A save from before map squares were counted per chain starts an open zone's squares at the whole game's (what it showed before) and a
    /// locked zone's at nothing, so only squares seen after its unlock count.
    fn normalize_counters(&mut self) {
        let migrate_cells = !self.counters.cells_counted;
        self.counters.cells_counted = true;
        for c in self.chains() {
            if migrate_cells && c.unit == ChainUnit::Cells && self.quest_unlocked(c.zone, &c.needs) {
                let seen = count_f64(self.fog.cells.len());
                let entry = self.counters.progress.entry(c.id.clone()).or_insert(0.0);
                *entry = entry.max(seen);
            }
            let floor = c.marks.iter().filter(|m| self.done.contains(&m.location_id)).map(|m| m.at).fold(0.0, f64::max);
            let entry = self.counters.progress.entry(c.id).or_insert(0.0);
            if *entry < floor {
                *entry = floor;
            }
        }
    }

    /// The `(id, name)` of every saved game under `dir`.
    #[must_use]
    pub fn list_ids(dir: &Path) -> Vec<(String, String)> {
        let mut out = Vec::new();
        if let Ok(rd) = std::fs::read_dir(dir.join("games")) {
            for e in rd.flatten().filter(|e| e.path().extension().is_some_and(|x| x == "json")) {
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
#[allow(clippy::assert_is_empty, clippy::cast_possible_wrap, clippy::cast_precision_loss)] // test code: `is_empty()` reads better in assertions than comparing with a typed empty array; test fixtures use small numbers
mod tests {
    use super::*;
    use crate::geo::destination;
    use crate::loc::Provider;
    use crate::realm::Shape;
    use crate::scan::build_atlas;
    use crate::solo::{generate, SoloOptions};

    #[test]
    fn only_lasting_progress_saves_the_game_at_once() {
        let at_once = [
            Event::QuestDone { location_id: 1, name: "q".into() },
            Event::SendCheck { location_id: 1 },
            Event::Reward { location_id: 1, item: "i".into() },
            Event::ZoneUnlocked { zone: 2 },
            Event::ShuffleRequested,
            Event::GoalAchieved { label: "g".into() },
            // A trap ending on a fix (thawed out, detour done, toll paid) is told as Info: lost, the player would redo it.
            Event::Info { text: "You thawed out.".into() },
        ];
        assert!(at_once.iter().all(Event::saves_at_once));
        // A reveal comes back on the next walk past, a trap is re-sent by the server from the saved item index: the periodic save
        // picks them up.
        let later = [Event::Discovered { location_id: 1 }, Event::Trap { item: "t".into(), message: "m".into(), alert: true }];
        assert!(!later.iter().any(Event::saves_at_once));
    }

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
        let mut a = build_atlas(id, 0, vec![], streets, &Catalog::builtin());
        a.ways = crate::loc::bench::grid_ways(destination(destination(home(), 180.0, 1000.0), 270.0, 1000.0), 21, 100.0);
        (r, a)
    }

    /// A reach-only solo game with one zone per entry of `zones`; any `Run` entry turns run quests on.
    fn reach_only(zones: &[Mode], trips: u32, goal: &str) -> SoloOptions {
        SoloOptions {
            zone_count: count_u32(zones.len()),
            run_quests: zones.contains(&Mode::Run),
            number_of_trips: trips,
            goal: goal.into(),
            quest_types: vec!["reach".into()],
            ..SoloOptions::default()
        }
    }

    fn game(o: &SoloOptions, backend: Backend, seed: u64) -> Game {
        let g = generate(o, seed).unwrap();
        let realms: Vec<(Realm, Atlas)> = (0..o.zone_count).map(|i| realm(&format!("r{i}"), Mode::Walk)).collect();
        let zr = (0..o.zone_count).map(|i| format!("r{i}")).collect();
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
                allow_parking: true,
            },
            &Catalog::builtin(),
        )
        .unwrap()
    }

    fn fixat(p: Point, t_s: i64) -> Estimate {
        Estimate::exact(p.lat, p.lon, t_s * 1000)
    }

    fn raw(p: Point, t_s: i64, acc: f64) -> RawFix {
        RawFix::at(p.lat, p.lon, t_s * 1000, acc)
    }

    #[test]
    fn relink_fills_the_place_type_for_old_and_already_linked_quests() {
        let cat = Catalog::builtin();
        let mut g = game(&reach_only(&[Mode::Walk], 10, "all_trips"), Backend::Solo, 4);
        let spot = destination(home(), 0.0, 300.0);
        let art = crate::scan::Feature {
            id: "n5".into(),
            point: spot,
            name: Some("Allow Me".into()),
            geometry: vec![],
            tags: [("tourism", "artwork"), ("name", "Allow Me"), ("wikipedia", "en:Allow Me")]
                .iter()
                .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
                .collect(),
        };
        let atlas = build_atlas("r0", 0, vec![art], vec![], &cat);
        // One quest saved before feature ids, one linked by an earlier build that did not keep the place type.
        for (i, linked) in [(0, None), (1, Some("n5".to_string()))] {
            let a = &mut g.assignments[i];
            (a.kind_id, a.place, a.feature_id, a.place_type, a.has_about) = ("gallery_walls".into(), "Allow Me".into(), linked, None, false);
            a.target = Target::Point { p: spot, r: 40.0 };
        }
        assert_eq!(g.relink_places(&[&atlas], &cat), 2);
        for i in 0..2 {
            let a = &g.assignments[i];
            assert_eq!((a.feature_id.as_deref(), a.place_type.as_deref(), a.has_about), (Some("n5"), Some("Artwork"), true), "quest {i}");
        }
        assert_eq!(g.relink_places(&[&atlas], &cat), 0, "nothing left to fill");
    }

    #[test]
    fn a_slow_trap_shows_the_doubled_dwell_on_the_card() {
        let mut g = game(&reach_only(&[Mode::Walk], 10, "all_trips"), Backend::Solo, 4);
        let id = g.assignments[0].location_id;
        g.assignments[0].target = Target::Dwell { p: home(), r: 40.0, minutes: 3.0 };
        let minutes = |g: &Game| match g.quest_views(0).into_iter().find(|q| q.location_id == id).unwrap().target {
            Target::Dwell { minutes, .. } => minutes,
            _ => unreachable!(),
        };
        assert!((minutes(&g) - 3.0).abs() < 1e-9);
        g.traps.trigger("Slow Trap", 0, Some(g.home), g.home, &g.trap_paths(), &mut SeedableRng::seed_from_u64(1));
        assert!((minutes(&g) - 6.0).abs() < 1e-9, "the card counts against the time the tracker needs");
    }

    #[test]
    fn relink_refreshes_what_a_rescan_changed_about_a_linked_place() {
        let cat = Catalog::builtin();
        let mut g = game(&reach_only(&[Mode::Walk], 10, "all_trips"), Backend::Solo, 4);
        let spot = destination(home(), 0.0, 300.0);
        let tags = |t: &[(&str, &str)]| t.iter().map(|(k, v)| ((*k).to_string(), (*v).to_string())).collect();
        let a = &mut g.assignments[0];
        (a.feature_id, a.place_type, a.has_about) = (Some("n5".into()), Some("Artwork".into()), true);
        // Rescanned: the artwork lost its Wikipedia tag, so there is no more to tell.
        let bare = crate::scan::Feature { id: "n5".into(), point: spot, name: None, geometry: vec![], tags: tags(&[("tourism", "artwork")]) };
        assert_eq!(g.relink_places(&[&build_atlas("r0", 0, vec![bare], vec![], &cat)], &cat), 1);
        assert!(!g.assignments[0].has_about);
        // Rescanned again: the place is gone from the map; the quest keeps its type but has nothing to show.
        g.assignments[0].has_about = true;
        assert_eq!(g.relink_places(&[&build_atlas("r0", 0, vec![], vec![], &cat)], &cat), 1);
        assert_eq!((g.assignments[0].place_type.as_deref(), g.assignments[0].has_about), (Some("Artwork"), false));
        assert_eq!(g.relink_places(&[&build_atlas("r0", 0, vec![], vec![], &cat)], &cat), 0, "settled");
    }

    #[test]
    fn relink_does_not_count_a_place_with_no_type_again_and_again() {
        let cat = Catalog::builtin();
        let mut g = game(&reach_only(&[Mode::Walk], 10, "all_trips"), Backend::Solo, 4);
        let spot = destination(home(), 0.0, 300.0);
        let odd = crate::scan::Feature { id: "n6".into(), point: spot, name: None, geometry: vec![], tags: BTreeMap::new() };
        let atlas = build_atlas("r0", 0, vec![odd], vec![], &cat);
        let a = &mut g.assignments[0];
        (a.feature_id, a.place_type) = (Some("n6".into()), None);
        assert_eq!(g.relink_places(&[&atlas], &cat), 0, "nothing changed, so nothing to save");
    }

    #[test]
    fn quest_feature_names_the_zone_realm_and_place() {
        let mut g = game(&reach_only(&[Mode::Walk, Mode::Walk], 10, "all_trips"), Backend::Solo, 4);
        let (first, other) = (g.assignments[0].location_id, g.assignments[1].location_id);
        g.assignments[0].feature_id = Some("n7".into());
        let zone = g.assignments[0].zone;
        let realm = format!("r{}", g.slot.zones.iter().position(|z| z.id == zone).unwrap());
        assert_eq!(g.quest_feature(first), Some((realm.as_str(), "n7")));
        assert_eq!(g.quest_feature(other), None, "a quest with no place");
        assert_eq!(g.quest_feature(-1), None, "no such quest");
        assert!(g.quest_views(0).iter().any(|q| q.location_id == first && q.feature_id.as_deref() == Some("n7")));
    }

    #[test]
    fn a_new_game_starts_at_once_without_a_street_graph_and_gets_it_later() {
        // Owner ruling (round 3): making or opening a game never waits for the graph; it is built in the background and swapped in.
        let mut g = game(&reach_only(&[Mode::Walk], 10, "all_trips"), Backend::Solo, 4);
        assert!(g.street_graph().is_none(), "no graph yet");
        assert!(!g.streets_attached(), "nor a street index: the background refresh builds both once (final review M6)");
        g.on_fix_with_zone(&raw(home(), 1, 5.0), None, |_| {});
        assert!(g.last_est.is_some_and(|e| e.accepted), "a fix is processed normally without the graph");
        let (_, a) = realm("r0", Mode::Walk);
        assert!(g.set_streets(Game::build_streets(&[&a]), 1));
        assert!(g.street_graph().is_some_and(|gr| !gr.is_degraded()) && g.streets_attached());
    }

    #[test]
    fn a_stale_street_build_never_replaces_a_newer_one() {
        let mut g = game(&reach_only(&[Mode::Walk], 10, "all_trips"), Backend::Solo, 4);
        let (_, mut small) = realm("r0", Mode::Walk);
        small.ways.truncate(2);
        let (_, full) = realm("r0", Mode::Walk);
        let (older, newer) = (Game::build_streets(&[&small]), Game::build_streets(&[&full]));
        let want = newer.1.as_ref().map(|gr| gr.segment_count());
        assert!(g.set_streets(newer, 2), "generation 2 goes in");
        assert!(!g.set_streets(older, 1), "generation 1 finished later but is older: dropped");
        assert_eq!(g.street_graph().map(|gr| gr.segment_count()), want);
        g.attach_streets(&[&small]);
        assert_eq!(g.street_graph().map(|gr| gr.segment_count()), Some(2 * 20), "attaching now is always newest");
    }

    #[test]
    fn an_opened_game_gets_its_street_graph_when_streets_are_attached() {
        let g = game(&reach_only(&[Mode::Walk], 10, "all_trips"), Backend::Solo, 4);
        let mut saved: Game = serde_json::from_str(&serde_json::to_string(&g).unwrap()).unwrap();
        assert!(saved.locator.graph().is_none(), "the graph is never saved");
        let (_, a) = realm("r0", Mode::Walk);
        saved.attach_streets(&[&a]);
        assert!(saved.locator.graph().is_some_and(|gr| !gr.is_degraded()));
    }

    #[test]
    fn reach_targets_list_every_quest_with_a_point_to_reach() {
        let g = game(&reach_only(&[Mode::Walk], 10, "all_trips"), Backend::Solo, 4);
        let t = g.reach_targets();
        assert_eq!(t.len(), g.assignments.len(), "reach-only game: every quest has a point");
        assert!(t.iter().all(|(_, _, r)| *r > 0.0));
    }

    #[test]
    fn the_trap_pool_has_streets_and_paths_from_every_zone() {
        let (r0, a0) = realm("r0", Mode::Walk);
        let (r1, mut a1) = realm("r1", Mode::Walk);
        let far = destination(home(), 0.0, 30_000.0);
        let trail = destination(far, 0.0, 60.0);
        (a1.streets, a1.streets_rough) = (vec![far], vec![trail]);
        let zones = [ZoneCtx { zone: 1, realm: &r0, atlas: &a0 }, ZoneCtx { zone: 2, realm: &r1, atlas: &a1 }];
        let pool = trap_pool(&zones);
        assert!(pool.contains(&far) && pool.contains(&trail), "a trap in zone 2 finds zone 2's streets");
        assert!(pool.len() <= 2 * TRAP_POOL_PER_ZONE + 2, "kept small: {}", pool.len());
    }

    #[test]
    fn a_trap_finds_its_point_among_every_street_of_the_zones_not_only_the_saved_sample() {
        let o = reach_only(&[Mode::Walk], 6, "all_trips");
        let mut g = game(&o, Backend::Solo, 3);
        let (_, a) = realm("r0", Mode::Walk);
        let on_street = |p: Point| a.streets.iter().any(|s| distance_m(*s, p) < 1e-6);
        // an old save's thin pool has nothing near; the streets attached when the game is opened do
        g.trap_pool = vec![destination(home(), 0.0, 50_000.0)];
        let mut saved: Game = serde_json::from_str(&serde_json::to_string(&g).unwrap()).unwrap();
        assert!(!saved.streets_attached(), "the street index is never saved");
        saved.attach_streets(&[&a]);
        assert!(saved.streets_attached());
        for i in 0..5 {
            saved.receive_item("Freeze Trap", i64::from(i), Some(home()));
            let thaw = saved.traps.thaw_point().unwrap();
            assert!(on_street(thaw) && (300.0..=800.0).contains(&distance_m(home(), thaw)), "thaw {thaw:?}");
            saved.traps.active.clear();
        }
    }

    /// A solo game whose quests are replaced by the given targets (all in zone 1, kind `kind`), each rewarding "Hydrate!".
    fn chain_game(kind: &str, targets: Vec<Target>) -> Game {
        let mut g = game(&reach_only(&[Mode::Walk], 10, "all_trips"), Backend::Solo, 4);
        g.assignments = targets.into_iter().enumerate().map(|(i, t)| chain::tests_support::member(1000 + i as i64, 1, kind, t)).collect();
        g.done.clear();
        g.solo_rewards = g.assignments.iter().map(|a| (a.location_id, "Hydrate!".to_string())).collect();
        g
    }

    fn done_ids(ev: &[Event]) -> Vec<i64> {
        ev.iter().filter_map(|e| if let Event::QuestDone { location_id, .. } = e { Some(*location_id) } else { None }).collect()
    }

    #[test]
    fn chain_views_report_counter_marks_rewards_and_the_rule() {
        let mut g = chain_game("step_up", vec![Target::Steps { n: 500 }, Target::Steps { n: 1000 }]);
        g.on_steps(0, 1, None);
        g.on_steps(600, 2, None); // first mark reached
        let v = &g.chain_views(0)[0];
        assert_eq!((v.id.as_str(), v.total, v.counter), ("1:step_up", 1500.0, 600.0));
        assert_eq!(v.rule, "Take 1,500 steps");
        assert_eq!(v.marks.iter().map(|m| (m.at, m.reached, m.reward.is_some())).collect::<Vec<_>>(), vec![(500.0, true, true), (1500.0, false, false)]);
    }

    #[test]
    fn chain_members_carry_their_chain_id_and_their_own_progress() {
        let mut g = chain_game("step_up", vec![Target::Steps { n: 500 }, Target::Steps { n: 1000 }]);
        g.on_steps(0, 1, None);
        g.on_steps(1000, 2, None); // mark 1 done, 500 of the next 1000
        let views = g.quest_views(0);
        assert!(views.iter().all(|q| q.chain_id.as_deref() == Some("1:step_up")));
        let (a, b) = (&views[0], &views[1]);
        assert_eq!((a.state, a.progress), (QuestState::Done, 1.0));
        assert!((b.progress - 0.5).abs() < 0.01 && b.state == QuestState::InProgress, "{} {:?}", b.progress, b.state);
    }

    #[test]
    fn other_quests_have_no_chain_id() {
        let g = game(&reach_only(&[Mode::Walk], 10, "all_trips"), Backend::Solo, 4);
        assert!(g.quest_views(0).iter().all(|q| q.chain_id.is_none()));
    }

    #[test]
    fn a_milestone_is_logged_with_its_place_in_the_chain() {
        let mut g = chain_game("step_up", vec![Target::Steps { n: 500 }, Target::Steps { n: 1000 }]);
        g.on_steps(0, 1, None);
        let ev = g.on_steps(1600, 2, None);
        let log = g.journal_events(&ev, 2000, None);
        let done: Vec<&str> = log.iter().filter(|e| e.kind == "quest_done").map(|e| e.detail.as_str()).collect();
        assert_eq!(done, vec!["step up milestone 1 of 2: 500 steps", "step up milestone 2 of 2: 1,500 steps"]);
    }

    fn away_game(minutes: &[f64]) -> Game {
        chain_game("wanderlust", minutes.iter().map(|m| Target::Away { minutes: *m }).collect())
    }

    /// Fixes every 60 s at `dist` metres east of home, `n` of them.
    fn away_for(g: &mut Game, dist: f64, from_s: i64, n: i64) -> Vec<Event> {
        let p = destination(g.home, 90.0, dist);
        (0..n).flat_map(|i| g.on_estimate(&fixat(p, from_s + i * 60), None)).collect()
    }

    #[test]
    fn new_map_squares_unlock_marks_from_the_cells_the_game_has_seen() {
        let mut g = chain_game("cartographer", vec![Target::Cells { n: 3, cell_m: 150.0 }, Target::Cells { n: 2, cell_m: 150.0 }]); // marks at 2 and 5
        let mut done = Vec::new();
        for i in 0..8 {
            let p = destination(g.home, 90.0, 200.0 * f64::from(i)); // a new 150 m cell every fix
            done.extend(done_ids(&g.on_estimate(&fixat(p, 1000 + i64::from(i) * 150), None)));
        }
        assert_eq!(done, vec![1001, 1000], "two cells unlock the 2-cell member, five the next");
    }

    /// Two Cartographer members in zone 1: marks at 2 and 5 squares.
    fn cartographer_game() -> Game {
        chain_game("cartographer", vec![Target::Cells { n: 2, cell_m: 150.0 }, Target::Cells { n: 3, cell_m: 150.0 }])
    }

    /// Walk east into `n` new 150 m squares, starting at step `from` (steps are 200 m and 150 s apart, so keep `from` increasing).
    fn new_squares(g: &mut Game, from: i32, n: i32) -> Vec<i64> {
        let mut done = Vec::new();
        for i in from..from + n {
            let p = destination(g.home, 90.0, 200.0 * f64::from(i));
            done.extend(done_ids(&g.on_estimate(&fixat(p, 1000 + i64::from(i) * 150), None)));
        }
        done
    }

    fn cartographer_counter(g: &Game) -> f64 {
        g.chain_views(0).into_iter().find(|c| c.id == "1:cartographer").unwrap().counter
    }

    #[test]
    fn squares_in_a_locked_zone_do_not_count_and_only_new_squares_count_after_unlock() {
        let mut g = cartographer_game();
        lock_zone_1(&mut g);
        assert!(new_squares(&mut g, 0, 6).is_empty());
        assert_eq!(g.fog.cells.len(), 6, "the map still records every square");
        assert_eq!(cartographer_counter(&g), 0.0, "a locked zone's chain must not fill");
        assert!(done_ids(&g.receive_item("Progressive Zone Key", 2000, None)).is_empty(), "unlocking pays nothing out at once");
        assert_eq!(cartographer_counter(&g), 0.0);
        assert!(new_squares(&mut g, 6, 1).is_empty());
        assert_eq!(new_squares(&mut g, 7, 1), vec![1000], "the second new square reaches the 2-square mark");
        assert_eq!(cartographer_counter(&g), 2.0);
    }

    #[test]
    fn squares_seen_while_a_zone_is_relocked_do_not_count_and_nothing_pays_twice() {
        let mut g = cartographer_game();
        lock_zone_1(&mut g);
        let key = vec!["Progressive Zone Key".to_string()];
        g.sync_items(&key, 1, None);
        assert_eq!(new_squares(&mut g, 0, 2), vec![1000]);
        g.sync_items(&[], 2, None); // the server's list shrank: the zone locks again
        assert!(!g.zone_unlocked(1));
        assert!(new_squares(&mut g, 2, 5).is_empty());
        assert_eq!(cartographer_counter(&g), 2.0, "credited squares are kept, locked ones are not added");
        assert!(done_ids(&g.sync_items(&key, 3, None)).is_empty(), "re-unlocking pays nothing at once");
        assert!(g.done.contains(&1000), "a finished mark stays finished");
        assert!(new_squares(&mut g, 7, 2).is_empty());
        assert_eq!(new_squares(&mut g, 9, 1), vec![1001], "the 5-square mark pays once, after 3 more unlocked squares");
        assert!(new_squares(&mut g, 10, 3).is_empty(), "nothing pays twice");
    }

    #[test]
    fn zones_open_from_the_start_count_every_square_of_the_game() {
        let mut g = cartographer_game();
        assert_eq!(new_squares(&mut g, 0, 3), vec![1000]);
        assert_eq!(cartographer_counter(&g), 3.0);
        assert_eq!(cartographer_counter(&g), count_f64(g.fog.cells.len()));
    }

    #[test]
    fn a_bridged_estimate_uncovers_fog_and_squares_and_nothing_else() {
        let mut g = cartographer_game();
        let before = (g.stats.distance_m, g.counters.steps_last, g.last_pos(), g.done.clone());
        let far = destination(home(), 0.0, 2000.0);
        let b = Estimate { source: Source::Bridged, accepted: false, ..fixat(far, 100) };
        g.on_estimate(&b, None);
        assert!(g.fog.cells.contains(&crate::fog::cell_of(far)), "the square is seen");
        assert!(cartographer_counter(&g) > 0.0, "and Cartographer counts it");
        assert_eq!((g.stats.distance_m, g.counters.steps_last, g.last_pos(), g.done.clone()), before, "no distance, steps, position or quest");
    }

    #[test]
    fn a_bridged_walk_past_a_quest_never_completes_it() {
        // Task 22 note 7: a GPS gap bridged by steps walks right through a reach quest; only fog and squares may move.
        let (mut g, id, p0) = start_near_a_quest();
        let target = target_of(&g, id);
        let course = crate::geo::bearing_deg(p0, target);
        let at = |s: i64| destination(p0, course, 1.4 * i64_to_f64(s));
        let mut steps = 10_000.0_f64;
        for s in 1..=30 {
            g.on_fix(&raw(at(s), 1000 + s, 4.0), Some(round_i64(steps)));
            steps += 1.93;
        }
        let (distance, last) = (g.stats.distance_m, g.last_pos());
        let mut nearest = f64::INFINITY;
        for s in 31..=330 {
            steps += 1.93;
            g.on_steps(round_i64(steps), (1000 + s) * 1000, None);
            if let Some(d) = g.position((1000 + s) * 1000).filter(|d| d.source == crate::loc::DisplaySource::Bridged) {
                nearest = nearest.min(distance_m(Point::new(d.lat, d.lon), target));
            }
        }
        assert!(nearest < 20.0, "the bridged pin passed the quest: {nearest} m");
        assert!(!g.done.contains(&id), "a bridged position never completes a quest");
        assert_eq!((g.stats.distance_m, g.last_pos()), (distance, last), "nor adds distance or moves the accepted position");
    }

    #[test]
    fn the_first_fix_after_a_bridged_gap_never_completes_a_quest_from_the_bridge() {
        // Adversarial review C1: the bridged pin walks onto the target on steps alone, then the only fix after the gap is 12 m outside
        // the radius (30 m accuracy). The bridge re-anchors the filter, but its prior must not decide the quest.
        let (mut g, id, p0) = start_near_a_quest();
        let target = target_of(&g, id);
        let Target::Point { r, .. } = g.assignments.iter().find(|a| a.location_id == id).unwrap().target else { panic!("a point quest") };
        let course = crate::geo::bearing_deg(p0, target);
        let at = |s: i64| destination(p0, course, 1.4 * i64_to_f64(s));
        let mut steps = 10_000.0_f64;
        for s in 1..=30 {
            g.on_fix(&raw(at(s), 1000 + s, 4.0), Some(round_i64(steps)));
            g.on_steps(round_i64(steps), (1000 + s) * 1000, None);
            steps += 1.93;
        }
        let mut s = 31;
        let mut reached = false;
        while s < 300 && !reached {
            steps += 1.93;
            g.on_steps(round_i64(steps), (1000 + s) * 1000, None);
            reached = g
                .position((1000 + s) * 1000)
                .is_some_and(|d| d.source == crate::loc::DisplaySource::Bridged && distance_m(Point::new(d.lat, d.lon), target) < 3.0);
            s += 1;
        }
        assert!(reached, "the bridged pin reached the target");
        let gps = destination(target, course + 90.0, r + 12.0);
        g.on_fix(&raw(gps, 1000 + s + 1, 30.0), Some(round_i64(steps)));
        let e = g.last_estimate().unwrap();
        assert!(!e.accepted, "{e:?}");
        assert!(!g.done.contains(&id), "completed although the only fix is {:.1} m away (radius {r})", distance_m(gps, target));
    }

    #[test]
    fn an_old_save_keeps_its_squares_in_open_zones_and_drops_them_in_locked_ones() {
        let dir = std::env::temp_dir().join(format!("apgo-cells-oldsave-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        // Open zone: the old counter was the whole game's squares, and stays so (nothing lost, nothing paid twice).
        let mut g = cartographer_game();
        assert_eq!(new_squares(&mut g, 0, 3), vec![1000]);
        g.counters = Counters::default();
        g.save(&dir).unwrap();
        let mut back = Game::load(&dir, "g1").unwrap();
        assert_eq!(cartographer_counter(&back), 3.0);
        assert!(new_squares(&mut back, 3, 1).is_empty(), "the 2-square mark is not paid again");
        assert_eq!(new_squares(&mut back, 4, 1), vec![1001]);
        // Locked zone: squares seen before the unlock never count.
        let mut g = cartographer_game();
        lock_zone_1(&mut g);
        assert!(new_squares(&mut g, 0, 6).is_empty());
        g.counters = Counters::default();
        g.save(&dir).unwrap();
        let mut back = Game::load(&dir, "g1").unwrap();
        assert_eq!(cartographer_counter(&back), 0.0);
        assert!(done_ids(&back.receive_item("Progressive Zone Key", 2000, None)).is_empty(), "unlocking pays nothing out at once");
        assert!(new_squares(&mut back, 6, 1).is_empty());
        assert_eq!(new_squares(&mut back, 7, 1), vec![1000]);
        // A saved game of this version is not migrated again on the next load.
        back.save(&dir).unwrap();
        assert_eq!(cartographer_counter(&Game::load(&dir, "g1").unwrap()), 2.0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Lock zone 1 (where `chain_game` puts its members) behind one zone key.
    fn lock_zone_1(g: &mut Game) {
        g.slot.zones.iter_mut().find(|z| z.id == 1).unwrap().zone_keys_needed = 1;
        assert!(!g.zone_unlocked(1));
    }

    #[test]
    fn steps_in_a_locked_zone_do_not_count_and_only_new_steps_count_after_unlock() {
        let mut g = chain_game("step_up", vec![Target::Steps { n: 500 }, Target::Steps { n: 1000 }]); // marks at 500 and 1500
        lock_zone_1(&mut g);
        g.on_steps(0, 1, None);
        assert!(g.on_steps(5000, 2, None).is_empty());
        assert!(g.counters.progress.get("1:step_up").is_none_or(|n| *n == 0.0), "a locked zone's chain must not fill");
        assert!(done_ids(&g.receive_item("Progressive Zone Key", 3, None)).is_empty());
        assert!(g.on_steps(5300, 4, None).is_empty(), "unlocking pays nothing out at once");
        assert!((g.counters.progress["1:step_up"] - 300.0).abs() < 0.01);
        assert_eq!(done_ids(&g.on_steps(5600, 5, None)), vec![1000]);
    }

    #[test]
    fn minutes_away_in_a_locked_zone_do_not_count_and_only_new_minutes_count_after_unlock() {
        let mut g = away_game(&[3.0, 2.0]); // marks at 2 and 5 minutes
        lock_zone_1(&mut g);
        assert!(away_for(&mut g, 1500.0, 0, 10).is_empty());
        assert!(g.counters.progress.get("1:wanderlust").is_none_or(|m| *m == 0.0), "a locked zone's chain must not fill");
        assert!(done_ids(&g.receive_item("Progressive Zone Key", 560_000, None)).is_empty());
        assert!(away_for(&mut g, 1500.0, 600, 1).is_empty(), "unlocking pays nothing out at once");
        assert!((g.counters.progress["1:wanderlust"] - 40.0 / 60.0).abs() < 0.01, "only the 40 s after the unlock at 560 s count");
        assert!(away_for(&mut g, 1500.0, 660, 1).is_empty());
        assert_eq!(done_ids(&away_for(&mut g, 1500.0, 720, 1)), vec![1001]);
    }

    #[test]
    fn a_reward_that_unlocks_the_zone_does_not_credit_the_locked_interval_before_it() {
        let mut g = away_game(&[3.0, 2.0]); // marks at 2 and 5 minutes
        lock_zone_1(&mut g);
        g.slot.zones.push(crate::slot::ZoneSlot { id: 2, zone_keys_needed: 0 });
        let key_spot = destination(g.home, 90.0, 1600.0); // 100 m beyond where `away_for` stands
        g.assignments.push(chain::tests_support::member(2000, 2, "reach", Target::Point { p: key_spot, r: 50.0 }));
        g.solo_rewards.insert(2000, "Progressive Zone Key".into());
        away_for(&mut g, 1500.0, 0, 5); // locked, last fix at 240 s
        let ev = g.on_estimate(&fixat(key_spot, 300), None);
        assert!(ev.contains(&Event::ZoneUnlocked { zone: 1 }));
        assert!(g.counters.progress.get("1:wanderlust").is_none_or(|m| *m == 0.0), "the minute before the unlock was locked");
        g.on_estimate(&fixat(key_spot, 360), None);
        assert!((g.counters.progress["1:wanderlust"] - 1.0).abs() < 0.01);
    }

    /// Every quest of `g` becomes a run that needs Running Shoes.
    fn badge_all(g: &mut Game) {
        for a in &mut g.assignments {
            a.mode = Mode::Run;
            a.needs = vec!["Running Shoes".into()];
        }
    }

    #[test]
    fn a_badged_steps_chain_counts_nothing_until_the_shoes_arrive() {
        let mut g = chain_game("step_up", vec![Target::Steps { n: 500 }, Target::Steps { n: 1000 }]);
        badge_all(&mut g);
        let id = "1:step_up:Running Shoes";
        assert_eq!(g.chain_views(0)[0].missing, vec!["Running Shoes".to_string()]);
        g.on_steps(0, 1, None);
        assert!(g.on_steps(5000, 2, None).is_empty());
        assert!(g.counters.progress.get(id).is_none_or(|n| *n == 0.0), "a badged chain must not fill");
        assert!(done_ids(&g.receive_item("Running Shoes", 3, None)).is_empty());
        assert!(g.chain_views(0)[0].missing.is_empty());
        g.on_steps(5300, 4, None);
        assert!((g.counters.progress[id] - 300.0).abs() < 0.01, "only steps after the shoes count");
    }

    #[test]
    fn the_shoes_do_not_credit_a_run_away_chain_with_time_from_before_they_arrived() {
        let mut g = away_game(&[3.0, 2.0]); // marks at 2 and 5 minutes
        badge_all(&mut g);
        let shoes_spot = destination(g.home, 90.0, 1600.0); // 100 m beyond where `away_for` stands
        g.assignments.push(chain::tests_support::member(2000, 1, "reach", Target::Point { p: shoes_spot, r: 50.0 }));
        g.solo_rewards.insert(2000, "Running Shoes".into());
        away_for(&mut g, 1500.0, 0, 5); // locked by the badge, last fix at 240 s
        g.on_estimate(&fixat(shoes_spot, 300), None);
        assert!(g.items.contains(&"Running Shoes".to_string()));
        let id = "1:wanderlust:Running Shoes";
        assert!(g.counters.progress.get(id).is_none_or(|m| *m == 0.0), "the time before the shoes was locked");
        g.on_estimate(&fixat(shoes_spot, 360), None);
        assert!((g.counters.progress[id] - 1.0).abs() < 0.01);
    }

    #[test]
    fn a_run_quest_at_its_target_is_not_sent_until_the_shoes_arrive() {
        let (mut g, id, _) = start_near_a_quest();
        badge_all(&mut g);
        let target = target_of(&g, id);
        let ev = g.on_estimate(&fixat(target, 1100), None);
        assert!(!done_ids(&ev).contains(&id), "a badged quest never completes without its badge");
        let nm = g.explain_near(&fixat(target, 1101), 100.0).into_iter().find(|n| n.location_id == id).expect("near");
        assert_eq!(nm.reason, "needs Running Shoes");
        g.receive_item("Running Shoes", 1_102_000, None);
        assert!(done_ids(&g.on_estimate(&fixat(target, 1200), None)).contains(&id));
    }

    #[test]
    fn steps_stop_counting_when_the_server_item_list_shrinks_and_relocks_the_zone() {
        let mut g = chain_game("step_up", vec![Target::Steps { n: 500 }, Target::Steps { n: 1000 }]);
        lock_zone_1(&mut g);
        g.receive_item("Progressive Zone Key", 0, None);
        g.on_steps(0, 1, None);
        g.on_steps(100, 2, None);
        g.sync_items(&[], 3, None);
        assert!(!g.zone_unlocked(1));
        g.on_steps(400, 4, None);
        assert!((g.counters.progress["1:step_up"] - 100.0).abs() < 0.01);
    }

    #[test]
    fn steps_count_from_the_first_reading_and_unlock_marks_in_order() {
        let mut g = chain_game("step_up", vec![Target::Steps { n: 500 }, Target::Steps { n: 1000 }]); // marks at 500 and 1500
        assert!(g.on_steps(10_000, 1, None).is_empty(), "the first reading only sets the baseline");
        assert!(g.on_steps(10_400, 2, None).is_empty());
        assert_eq!(done_ids(&g.on_steps(10_520, 3, None)), vec![1000]);
        assert_eq!(done_ids(&g.on_steps(11_600, 4, None)), vec![1001]);
        assert!(g.on_steps(20_000, 5, None).is_empty(), "nothing is paid twice");
        assert_eq!(g.counters.progress["1:step_up"], 10_000.0);
    }

    #[test]
    fn a_reboot_that_resets_the_counter_credits_the_new_reading_and_never_goes_negative() {
        let mut g = chain_game("step_up", vec![Target::Steps { n: 100_000 }]);
        g.on_steps(50_000, 1, None);
        g.on_steps(50_200, 2, None);
        g.on_steps(300, 3, None); // rebooted: the counter started again from zero
        assert_eq!(g.counters.progress["1:step_up"], 500.0);
        assert_eq!(g.counters.steps_last, Some(300));
    }

    #[test]
    fn steps_taken_while_the_game_is_closed_are_not_credited() {
        let dir = std::env::temp_dir().join(format!("apgo-steps-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut g = chain_game("step_up", vec![Target::Steps { n: 100_000 }]);
        g.on_steps(1_000, 1, None);
        g.on_steps(1_300, 2, None);
        g.save(&dir).unwrap();
        let mut back = Game::load(&dir, "g1").unwrap();
        assert_eq!(back.counters.progress["1:step_up"], 300.0, "progress is kept");
        assert_eq!(back.counters.steps_last, None, "the session baseline is not");
        back.on_steps(90_000, 3, None); // a whole day of walking with the game closed
        assert_eq!(back.counters.progress["1:step_up"], 300.0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn freeze(g: &mut Game) {
        g.traps.trigger("Freeze Trap", 0, Some(g.home), g.home, &g.trap_paths(), &mut SeedableRng::seed_from_u64(1));
        assert!(!g.traps.active.is_empty(), "the freeze trap is active");
    }

    #[test]
    fn steps_wait_under_a_trap_when_no_position_is_known_after_a_counting_toggle() {
        let mut g = chain_game("step_up", vec![Target::Steps { n: 500 }]);
        g.on_steps(1_000, 1, None);
        freeze(&mut g);
        g.set_counting(false, 0);
        g.set_counting(true, 0); // clears the last fix
        assert!(g.last_pos().is_none());
        g.on_steps(1_100, 2, None);
        assert!(g.on_steps(1_700, 3, None).is_empty(), "a frozen player earns nothing from steps");
        g.traps.active.clear();
        assert_eq!(done_ids(&g.on_steps(1_710, 4, None)), vec![1000], "the mark pays once the trap is gone");
    }

    #[test]
    fn steps_wait_under_a_saved_trap_right_after_loading() {
        let dir = std::env::temp_dir().join(format!("apgo-steps-trap-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut g = chain_game("step_up", vec![Target::Steps { n: 500 }]);
        freeze(&mut g);
        g.save(&dir).unwrap();
        let mut back = Game::load(&dir, "g1").unwrap();
        assert!(back.last_pos().is_none());
        back.on_steps(1_000, 1, None);
        assert!(back.on_steps(1_700, 2, None).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_fix_with_a_step_reading_also_credits_steps() {
        let mut g = chain_game("step_up", vec![Target::Steps { n: 500 }]);
        g.on_estimate(&fixat(home(), 1), Some(5_000));
        let ev = g.on_estimate(&fixat(home(), 20), Some(5_600));
        assert_eq!(done_ids(&ev), vec![1000]);
    }

    #[test]
    fn time_away_pauses_while_a_trap_blocks_checks() {
        let mut g = away_game(&[10.0]);
        away_for(&mut g, 1500.0, 0, 1);
        g.traps.trigger("Freeze Trap", 0, Some(g.home), g.home, &g.trap_paths(), &mut SeedableRng::seed_from_u64(1));
        away_for(&mut g, 1500.0, 60, 5);
        assert!(g.counters.progress.get("1:wanderlust").is_none_or(|m| *m == 0.0), "frozen: no minutes count");
        g.traps.active.clear();
        away_for(&mut g, 1500.0, 400, 4);
        assert!(g.counters.progress["1:wanderlust"] > 2.0, "minutes count again once the trap ends");
    }

    #[test]
    fn marks_wait_while_a_trap_blocks_checks_and_pay_when_it_ends() {
        let mut g = chain_game("step_up", vec![Target::Steps { n: 500 }]);
        g.on_estimate(&fixat(home(), 1), Some(1_000));
        g.traps.trigger("Freeze Trap", 0, Some(home()), home(), &g.trap_paths(), &mut SeedableRng::seed_from_u64(1));
        assert!(g.on_steps(1_600, 2, None).is_empty(), "frozen: no check counts");
        g.traps.active.clear();
        assert_eq!(done_ids(&g.on_steps(1_601, 3, None)), vec![1000], "the counter kept the steps");
    }

    /// Teleport (long gaps so speed gating does not apply) to every open quest until none remain.
    fn play_all(g: &mut Game, mut t: i64) -> Vec<Event> {
        let mut all = Vec::new();
        for _ in 0..500 {
            let Some(q) = g.quest_views(0).into_iter().find(|q| matches!(q.state, QuestState::Open | QuestState::InProgress)) else { break };
            t += 1000;
            all.extend(g.on_estimate(&fixat(q.anchor.expect("reach quests have a point"), t), None));
        }
        all
    }

    #[test]
    fn a_new_solo_game_assigns_everything_and_locks_later_zones() {
        let g = game(&reach_only(&[Mode::Walk, Mode::Walk], 20, "all_trips"), Backend::Solo, 4);
        assert_eq!(g.assignments.len(), 20);
        let views = g.quest_views(0);
        assert!(views.iter().filter(|v| v.zone == 2).all(|v| v.state == QuestState::Locked));
        assert!(views.iter().filter(|v| v.zone == 1).all(|v| v.state == QuestState::Open));
    }

    #[test]
    fn a_run_quest_is_locked_until_running_shoes_arrive() {
        let mut g = game(&reach_only(&[Mode::Run], 20, "all_trips"), Backend::Solo, 4);
        let shoes = vec!["Running Shoes".to_string()];
        let views = g.quest_views(0);
        let runs: Vec<&QuestView> = views.iter().filter(|v| v.mode == Mode::Run).collect();
        assert_eq!(runs.len(), 4, "20 % of 20");
        assert!(runs.iter().all(|v| v.state == QuestState::Locked && v.needs == shoes && v.missing == shoes));
        assert!(views.iter().filter(|v| v.mode == Mode::Walk).all(|v| v.state == QuestState::Open && v.needs.is_empty() && v.missing.is_empty()));
        assert!(!g.quest_unlocked(1, &shoes) && g.quest_unlocked(1, &[]));
        let missing = BadgeView { item: "Running Shoes".into(), icon: "run".into(), missing: true };
        let run = runs[0];
        assert_eq!((run.badges.clone(), run.pin_badge.clone()), (vec![missing.clone()], Some(missing.clone())));
        assert_eq!(run.badge_note.as_deref(), Some("Needs Running Shoes"));
        let walk = views.iter().find(|v| v.mode == Mode::Walk).unwrap();
        assert!(walk.badges.is_empty() && walk.pin_badge.is_none() && walk.badge_note.is_none());
        g.items.push("Running Shoes".into());
        let views = g.quest_views(0);
        assert!(views.iter().all(|v| v.state == QuestState::Open && v.missing.is_empty()), "the shoes open every run quest");
        let run = views.iter().find(|v| v.mode == Mode::Run).unwrap();
        assert_eq!(run.pin_badge, Some(BadgeView { missing: false, ..missing }), "a held badge still shows, navy");
        assert!(run.badge_note.is_none());
        assert!(g.quest_unlocked(1, &shoes));
    }

    #[test]
    fn the_pin_shows_the_first_missing_badge_and_the_note_names_them_all() {
        let mut g = game(&reach_only(&[Mode::Walk], 10, "all_trips"), Backend::Solo, 4);
        let two = vec!["Running Shoes".to_string(), "Gloves".to_string()];
        assert_eq!(g.badge_note(&two).as_deref(), Some("Needs Running Shoes and Gloves"));
        g.items.push("Running Shoes".into());
        let badges = g.badge_views(&two);
        assert_eq!(badges.iter().map(|b| (b.icon.as_str(), b.missing)).collect::<Vec<_>>(), vec![("run", false), ("tool", true)]);
        g.assignments[0].needs = two;
        let v = g.quest_views(0).into_iter().find(|v| v.location_id == g.assignments[0].location_id).unwrap();
        assert_eq!(v.pin_badge.map(|b| b.item).as_deref(), Some("Gloves"), "the missing one first");
        assert_eq!(v.badge_note.as_deref(), Some("Needs Gloves"));
    }

    #[test]
    fn a_locked_quest_never_completes_whatever_path_finishes_it() {
        let mut g = game(&reach_only(&[Mode::Walk], 10, "all_trips"), Backend::Solo, 4);
        badge_all(&mut g);
        let id = g.assignments[0].location_id;
        assert!(g.complete(id, 1_000, None).is_empty(), "a tracker that finishes after the badge was taken back sends nothing");
        assert!(!g.done.contains(&id));
        g.items.push("Running Shoes".into());
        assert!(done_ids(&g.complete(id, 2_000, None)).contains(&id));
    }

    #[test]
    fn the_travel_mode_follows_quests_that_can_be_played_not_just_held_badges() {
        let mut g = game(&reach_only(&[Mode::Walk, Mode::Walk], 20, "all_trips"), Backend::Solo, 4);
        for a in g.assignments.iter_mut().filter(|a| a.zone == 2) {
            a.mode = Mode::Run;
            a.needs = vec!["Running Shoes".into()];
        }
        g.receive_item("Running Shoes", 0, None);
        assert_eq!(g.locator.mode(), Mode::Walk, "every run quest is in locked zone 2");
        g.sync_items(&["Running Shoes".into(), "Progressive Zone Key".into()], 1, None);
        assert_eq!(g.locator.mode(), Mode::Run, "the key opens them");
    }

    #[test]
    fn a_server_rollback_keeps_the_time_away_earned_before_it() {
        let mut g = away_game(&[30.0]);
        badge_all(&mut g);
        g.receive_item("Running Shoes", 0, None);
        away_for(&mut g, 1500.0, 0, 3); // fixes at 0, 60 and 120 s
        g.sync_items(&[], 150_000, None);
        let id = "1:wanderlust:Running Shoes";
        assert!((g.counters.progress[id] - 2.5).abs() < 0.01, "credited up to the rollback: {}", g.counters.progress[id]);
        away_for(&mut g, 1500.0, 180, 2);
        assert!((g.counters.progress[id] - 2.5).abs() < 0.01, "nothing after it");
    }

    #[test]
    fn a_quest_with_two_badges_needs_both() {
        let mut g = game(&reach_only(&[Mode::Walk], 10, "all_trips"), Backend::Solo, 4);
        let two = vec!["Running Shoes".to_string(), "Gloves".to_string()];
        assert_eq!(g.missing_needs(&two), two);
        g.items.push("Gloves".into());
        assert!(!g.quest_unlocked(1, &two));
        assert_eq!(g.missing_needs(&two), vec!["Running Shoes".to_string()]);
        g.items.push("Running Shoes".into());
        assert!(g.quest_unlocked(1, &two) && g.missing_needs(&two).is_empty());
        assert!(!g.quest_unlocked(9, &[]), "an unknown zone is never open");
    }

    #[test]
    fn quest_views_carry_the_kind_id_so_the_ui_can_pick_an_icon() {
        let g = game(&reach_only(&[Mode::Walk], 5, "all_trips"), Backend::Solo, 4);
        for (v, a) in g.quest_views(0).iter().zip(&g.assignments) {
            assert!(!v.kind_id.is_empty());
            assert_eq!(v.kind_id, a.kind_id);
        }
    }

    #[test]
    fn reaching_a_quest_completes_it_once_and_pays_the_solo_reward() {
        let mut g = game(&reach_only(&[Mode::Walk], 10, "all_trips"), Backend::Solo, 4);
        let q = g.quest_views(0).remove(0);
        let ev = g.on_estimate(&fixat(q.anchor.unwrap(), 100), None);
        assert!(ev.iter().any(|e| matches!(e, Event::QuestDone { location_id, .. } if *location_id == q.location_id)));
        assert!(ev.iter().any(|e| matches!(e, Event::Reward { .. })));
        assert!(g.on_estimate(&fixat(q.anchor.unwrap(), 200), None).iter().all(|e| !matches!(e, Event::QuestDone { .. })), "no double completion");
        assert_eq!(g.quest_views(0).iter().filter(|v| v.state == QuestState::Done).count(), 1);
    }

    #[test]
    fn full_playthrough_unlocks_zones_and_reports_the_goal_exactly_once() {
        let mut g = game(&reach_only(&[Mode::Walk, Mode::Run, Mode::Walk], 30, "all_trips"), Backend::Solo, 9);
        let ev = play_all(&mut g, 0);
        assert!(ev.iter().filter(|e| matches!(e, Event::ZoneUnlocked { .. })).count() >= 2, "zones 2 and 3 unlock along the way");
        assert_eq!(ev.iter().filter(|e| matches!(e, Event::GoalAchieved { .. })).count(), 1);
        assert_eq!(g.done.len(), 30);
        assert!(g.goal_status().achieved);
    }

    #[test]
    fn archipelago_backend_sends_checks_and_unlocks_from_synced_items() {
        let mut g = game(&reach_only(&[Mode::Walk, Mode::Run], 20, "all_trips"), Backend::Archipelago, 2);
        let q = g.quest_views(0).into_iter().find(|v| v.zone == 1).unwrap();
        let ev = g.on_estimate(&fixat(q.anchor.unwrap(), 50), None);
        assert!(ev.iter().any(|e| matches!(e, Event::SendCheck { .. })) && !ev.iter().any(|e| matches!(e, Event::Reward { .. })));
        let ev = g.sync_items(&["Progressive Zone Key".into(), "Running Shoes".into()], 60, None);
        assert!(ev.contains(&Event::ZoneUnlocked { zone: 2 }));
        let again = g.sync_items(&["Progressive Zone Key".into(), "Running Shoes".into()], 70, None);
        assert!(again.is_empty(), "already-seen items trigger nothing");
        assert!(g.quest_views(0).iter().any(|v| v.zone == 2 && v.state == QuestState::Open));
    }

    #[test]
    fn a_silence_trap_alerts_as_it_lands_then_mutes_the_alerts_of_later_traps_until_it_ends() {
        let mut g = game(&reach_only(&[Mode::Walk], 10, "all_trips"), Backend::Archipelago, 4);
        let alert = |ev: Vec<Event>| ev.into_iter().find_map(|e| if let Event::Trap { alert, .. } = e { Some(alert) } else { None });
        assert_eq!(alert(g.receive_item("Fog Of War Trap", 0, Some(home()))), Some(true));
        assert_eq!(alert(g.receive_item("Silence Trap", 10, Some(home()))), Some(true), "its own alert still fires");
        assert_eq!(alert(g.receive_item("Slow Trap", 20, Some(home()))), Some(false));
        assert_eq!(alert(g.receive_item("Freeze Trap", 10 + 15 * 60_000, Some(home()))), Some(true), "15 min later, with no fix between");
    }

    #[test]
    fn freeze_trap_blocks_progress_until_you_reach_the_thaw_point() {
        let mut g = game(&reach_only(&[Mode::Walk], 10, "all_trips"), Backend::Archipelago, 4);
        let ev = g.receive_item("Freeze Trap", 0, Some(home()));
        assert!(ev.iter().any(|e| matches!(e, Event::Trap { .. })));
        let thaw = g.traps.thaw_point().unwrap();
        let q = g.quest_views(0).remove(0);
        let ev = g.on_estimate(&fixat(q.anchor.unwrap(), 100), None);
        assert!(!ev.iter().any(|e| matches!(e, Event::QuestDone { .. })), "frozen: the check must not count");
        g.on_estimate(&fixat(thaw, 200), None);
        assert!(g.traps.thaw_point().is_none());
        // long gap: a teleport this far in under two minutes would (correctly) look like driving to a walking quest
        let ev = g.on_estimate(&fixat(q.anchor.unwrap(), 1000), None);
        assert!(ev.iter().any(|e| matches!(e, Event::QuestDone { .. })));
    }

    #[test]
    fn fog_hides_far_quests_and_discovery_reveals_them() {
        let mut o = reach_only(&[Mode::Walk], 20, "all_trips");
        o.fog_of_war = true;
        let mut g = game(&o, Backend::Solo, 6);
        assert!(g.quest_views(0).iter().filter(|v| v.state == QuestState::Hidden).count() >= 10, "most quests start hidden");
        let target = g.quest_views(0).into_iter().find(|v| v.state == QuestState::Hidden).unwrap();
        let ev = g.on_estimate(&fixat(destination(target.anchor.unwrap(), 0.0, 120.0), 10), None);
        assert!(ev.iter().any(|e| matches!(e, Event::Discovered { location_id } if *location_id == target.location_id)));
        assert!(matches!(g.quest_views(0).into_iter().find(|v| v.location_id == target.location_id).unwrap().state, QuestState::Open));
    }

    #[test]
    fn any_realm_can_serve_zones_of_any_mode_because_travel_is_the_games_choice() {
        let o = reach_only(&[Mode::Walk, Mode::Run], 12, "all_trips");
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
                allow_parking: true,
            },
            &Catalog::builtin(),
        );
        assert!(created.is_ok());
    }

    #[test]
    fn chain_members_cannot_be_rerolled_and_counters_survive_a_reroll() {
        let mut g = chain_game("step_up", vec![Target::Steps { n: 500 }, Target::Steps { n: 1000 }]);
        g.on_steps(0, 1, None);
        g.on_steps(400, 2, None);
        let before = g.assignments.iter().map(|a| a.target.clone()).collect::<Vec<_>>();
        let realms = vec![realm("r0", Mode::Walk)];
        let n = g.reroll(&[1000, 1001], &realms, 9, &Catalog::builtin()).unwrap();
        assert_eq!(n, 0, "nothing re-placed");
        assert_eq!(g.assignments.iter().map(|a| a.target.clone()).collect::<Vec<_>>(), before);
        assert_eq!(g.counters.progress["1:step_up"], 400.0);
    }

    #[test]
    fn an_old_save_with_finished_chain_quests_keeps_them_and_earns_nothing_twice() {
        let dir = std::env::temp_dir().join(format!("apgo-oldsave-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut g = chain_game("step_up", vec![Target::Steps { n: 500 }, Target::Steps { n: 1000 }, Target::Steps { n: 2000 }]);
        g.done.insert(1000); // finished the old way, before chains existed
        g.done.insert(1001);
        g.counters = Counters::default();
        g.save(&dir).unwrap();
        let mut back = Game::load(&dir, "g1").unwrap();
        assert!(back.done.contains(&1000) && back.done.contains(&1001), "nothing lost");
        assert_eq!(back.counters.progress["1:step_up"], 1500.0, "the counter starts at the highest finished mark");
        back.on_steps(10, 1, None);
        let ev = back.on_steps(600, 2, None);
        assert!(done_ids(&ev).is_empty(), "the 3rd mark is at 3,500; 590 more steps do not pay anything twice");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn reroll_skips_real_chain_members_in_a_generated_game() {
        let mut g = game(&reach_only(&[Mode::Walk], 10, "all_trips"), Backend::Solo, 4);
        let ids: Vec<i64> = g.assignments.iter().take(2).map(|a| a.location_id).collect();
        for (a, n) in g.assignments.iter_mut().take(2).zip([500, 1000]) {
            *a = chain::tests_support::member(a.location_id, 1, "step_up", Target::Steps { n });
        }
        let before = g.assignments.iter().map(|a| a.target.clone()).collect::<Vec<_>>();
        let realms = vec![realm("r0", Mode::Walk)];
        let n = g.reroll(&ids, &realms, 9, &Catalog::builtin()).unwrap();
        assert_eq!(n, 0, "chain members are never re-placed");
        assert_eq!(g.assignments.iter().map(|a| a.target.clone()).collect::<Vec<_>>(), before);
    }

    #[test]
    fn a_rerolled_quest_never_becomes_a_chain_member() {
        // Every family, so the steps / away / explore slots and the boss could all be re-placed as progressive kinds.
        let o = SoloOptions { zone_count: 1, number_of_trips: 20, goal: "all_trips".into(), ..SoloOptions::default() };
        let realms = vec![realm("r0", Mode::Walk)];
        let catalog = Catalog::builtin();
        for seed in 0..10 {
            let mut g = game(&o, Backend::Solo, seed);
            // Make every quest a plain one (as a fallback or an older version may have placed it), so each can be rerolled.
            for a in &mut g.assignments {
                a.target = Target::Point { p: home(), r: 40.0 };
            }
            let ids: Vec<i64> = g.assignments.iter().map(|a| a.location_id).collect();
            assert_eq!(g.reroll(&ids, &realms, seed, &catalog).unwrap(), ids.len(), "seed {seed}: every quest re-placed");
            let joined: Vec<&str> = g.assignments.iter().filter(|a| is_chain_target(&a.target)).map(|a| a.kind_id.as_str()).collect();
            assert!(joined.is_empty(), "seed {seed}: rerolled quests became chain members: {joined:?}");
        }
    }

    #[test]
    fn an_old_save_pays_the_next_mark_once_and_floors_a_minutes_chain() {
        let dir = std::env::temp_dir().join(format!("apgo-oldsave2-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut g = chain_game("step_up", vec![Target::Steps { n: 500 }, Target::Steps { n: 1000 }, Target::Steps { n: 1100 }]);
        g.done.extend([1000, 1001]);
        g.counters = Counters::default();
        g.save(&dir).unwrap();
        let mut back = Game::load(&dir, "g1").unwrap();
        back.on_steps(10, 1, None);
        let ev = back.on_steps(1210, 2, None); // 1,200 gained: 1,500 -> 2,700 crosses the mark at 2,600
        assert_eq!(done_ids(&ev), vec![1002]);

        let mut m = chain_game("wanderlust", vec![Target::Away { minutes: 30.0 }, Target::Away { minutes: 45.0 }]);
        m.done.insert(1000);
        m.counters = Counters::default();
        m.save(&dir).unwrap();
        let back = Game::load(&dir, "g1").unwrap();
        assert_eq!(back.counters.progress["1:wanderlust"], 30.0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn reroll_keeps_finished_quests() {
        let o = reach_only(&[Mode::Walk, Mode::Run], 12, "all_trips");
        let mut g = game(&o, Backend::Solo, 1);
        let q = g.quest_views(0).into_iter().find(|v| v.zone == 1).unwrap();
        g.on_estimate(&fixat(q.anchor.unwrap(), 1), None);
        let realms = vec![realm("r0", Mode::Walk), realm("r1", Mode::Run)];
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
        let q = g.quest_views(0).remove(0);
        g.on_estimate(&fixat(q.anchor.unwrap(), 5), None);
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
    fn a_save_whose_goal_was_removed_is_refused_with_a_clear_message() {
        let dir = std::env::temp_dir().join(format!("apgo-oldgoal-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut g = game(&reach_only(&[Mode::Walk], 10, "all_trips"), Backend::Solo, 4);
        g.slot.goals = vec!["boss".into()]; // The Big One, removed in Beta 1
        g.save(&dir).unwrap();
        let Err(err) = Game::load(&dir, "g1") else { panic!("a save with a removed goal must not load") };
        assert!(err.contains("older version") && err.contains("unknown goal boss"), "{err}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_save_with_a_bike_zone_is_refused() {
        let dir = std::env::temp_dir().join(format!("apgo-bikezone-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let g = game(&reach_only(&[Mode::Walk], 10, "all_trips"), Backend::Solo, 4);
        g.save(&dir).unwrap();
        let path = Game::path_for(&dir, "g1");
        let json = std::fs::read_to_string(&path).unwrap();
        let old = json.replace("\"mode\":\"walk\"", "\"mode\":\"bike\"");
        assert_ne!(old, json, "the save must hold a walk zone");
        std::fs::write(&path, old).unwrap();
        assert!(Game::load(&dir, "g1").is_err(), "Bike is gone; old saves do not load");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_schema_3_save_says_older_version_not_corrupt() {
        let dir = std::env::temp_dir().join(format!("apgo-schema3-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let g = game(&reach_only(&[Mode::Walk], 10, "all_trips"), Backend::Solo, 4);
        g.save(&dir).unwrap();
        let path = Game::path_for(&dir, "g1");
        let mut v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        v["slot"]["schema_version"] = 3.into();
        v["slot"]["goals"] = serde_json::json!([{ "id": "all_trips", "target": 0 }]);
        std::fs::write(&path, v.to_string()).unwrap();
        let Err(err) = Game::load(&dir, "g1") else { panic!("a schema 3 save must not load") };
        assert!(err.contains("older version") && err.contains("unsupported slot_data schema 3"), "{err}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn saving_is_atomic_and_leaves_no_temp_file_and_ignores_a_stale_one() {
        let dir = std::env::temp_dir().join(format!("apgo-atomic-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let g = game(&reach_only(&[Mode::Walk], 10, "all_trips"), Backend::Solo, 4);
        g.save(&dir).unwrap();
        let tmp = Game::path_for(&dir, "g1").with_extension("json.tmp");
        assert!(!tmp.exists(), "no temp file remains after a save");
        // a kill mid-write leaves a stale temp file (here: a full copy, the worst case for the list)
        std::fs::write(&tmp, std::fs::read_to_string(Game::path_for(&dir, "g1")).unwrap().replace("\"g1\"", "\"ghost\"")).unwrap();
        assert!(Game::load(&dir, "g1").is_ok(), "the real save still loads");
        assert_eq!(Game::list_ids(&dir).len(), 1, "a leftover temp file is not a game");
        std::fs::write(&tmp, "{half").unwrap();
        g.save(&dir).unwrap();
        assert!(!tmp.exists(), "the next save replaces the stale temp file");
        assert!(Game::load(&dir, "g1").is_ok());
        let _ = std::fs::remove_dir_all(&dir);
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

    #[test]
    fn nothing_counts_while_counting_is_off_and_no_steps_are_credited_for_that_time() {
        let mut g = chain_game("step_up", vec![Target::Steps { n: 500 }]);
        g.on_steps(1_000, 1, None);
        g.set_counting(false, 0);
        assert!(g.on_steps(5_000, 2, None).is_empty(), "4,000 steps at home");
        assert_eq!(g.counters.progress.get("1:step_up").copied().unwrap_or(0.0), 0.0);
        g.set_counting(true, 0);
        assert!(g.on_steps(5_100, 3, None).is_empty(), "only the 100 steps since the reading taken while leaving");
        assert_eq!(g.counters.progress["1:step_up"], 100.0);
    }

    #[test]
    fn steps_walked_while_leaving_are_backfilled_once_leaving_is_confirmed() {
        // Home Wi-Fi drops at 100 s: the phone starts the step counter, but counting waits for the 45 s debounce.
        let mut g = chain_game("step_up", vec![Target::Steps { n: 500 }]);
        g.on_steps(1_000, 0, None);
        g.set_counting(false, 10_000);
        assert!(g.on_steps(5_000, 100_000, None).is_empty(), "the reading at the door is the leave mark");
        assert!(g.on_steps(5_060, 130_000, None).is_empty());
        assert_eq!(g.counters.progress.get("1:step_up").copied().unwrap_or(0.0), 0.0, "nothing counts before it is confirmed");
        g.set_counting(true, 145_000);
        assert!(g.on_steps(5_100, 150_000, None).is_empty());
        assert_eq!(g.counters.progress["1:step_up"], 100.0, "every step since the door, the 45 s included");
    }

    #[test]
    fn an_old_leave_mark_from_a_wifi_blip_credits_nothing() {
        let mut g = chain_game("step_up", vec![Target::Steps { n: 500 }]);
        g.set_counting(false, 0);
        assert!(g.on_steps(5_000, 1_000, None).is_empty(), "Wi-Fi blipped: one reading, then the counter stopped again");
        g.set_counting(true, 4 * 3_600_000);
        assert!(g.on_steps(9_000, 4 * 3_600_000 + 5_000, None).is_empty(), "hours later the mark is stale: this is the baseline");
        assert_eq!(g.counters.progress.get("1:step_up").copied().unwrap_or(0.0), 0.0);
        assert!(g.on_steps(9_100, 4 * 3_600_000 + 10_000, None).is_empty());
        assert_eq!(g.counters.progress["1:step_up"], 100.0);
    }

    #[test]
    fn steps_walked_while_the_counter_was_off_at_home_are_not_credited() {
        // At home and in the car the phone stops the step counter: no reading at all while counting is off.
        let mut g = chain_game("step_up", vec![Target::Steps { n: 500 }]);
        g.on_steps(1_000, 1, None);
        g.set_counting(false, 0);
        g.set_counting(true, 0);
        assert!(g.on_steps(5_000, 2, None).is_empty(), "4,000 steps indoors are a baseline, not progress");
        assert_eq!(g.counters.progress.get("1:step_up").copied().unwrap_or(0.0), 0.0);
        assert!(g.on_steps(5_100, 3, None).is_empty());
        assert_eq!(g.counters.progress["1:step_up"], 100.0);
    }

    #[test]
    fn a_leave_mark_from_the_walk_out_replaces_steps_paced_indoors() {
        // With the home circle the counter runs at home: the core's home watch knows where the walk out began, and only steps from
        // there count, not the pacing indoors that set the game's own mark.
        let mut g = chain_game("step_up", vec![Target::Steps { n: 500 }]);
        g.set_counting(false, 0);
        g.on_steps(1_000, 10_000, None); // pacing indoors: the game's own mark
        g.set_leave_mark(1_300, 50_000); // the walk out began here
        g.set_counting(true, 80_000);
        g.on_steps(1_400, 90_000, None);
        assert_eq!(g.counters.progress["1:step_up"], 100.0);
        g.set_leave_mark(0, 95_000);
        g.set_counting(false, 100_000);
        g.set_counting(true, 101_000);
        g.on_steps(1_500, 102_000, None);
        assert_eq!(g.counters.progress["1:step_up"], 100.0, "a mark given while counting was on is not kept for the next pause");
    }

    #[test]
    fn a_dwell_started_before_a_pause_does_not_finish_on_the_first_fix_after_it() {
        let mut g = chain_game("dwell", vec![Target::Dwell { p: home(), r: 50.0, minutes: 10.0 }]);
        g.on_estimate(&fixat(home(), 0), None);
        g.on_estimate(&fixat(home(), 300), None); // 5 of 10 minutes
        g.set_counting(false, 0);
        g.set_counting(true, 0);
        let ev = g.on_estimate(&fixat(home(), 700), None);
        assert!(ev.is_empty() && g.done.is_empty(), "the timer starts again after the pause: {ev:?}");
        let ev = g.on_estimate(&fixat(home(), 1400), None);
        assert_eq!(done_ids(&ev), vec![1000], "and a full dwell after the pause still counts");
    }

    #[test]
    fn a_courier_pickup_survives_a_counting_pause() {
        let (a, b) = (destination(home(), 0.0, 400.0), destination(home(), 90.0, 800.0));
        let mut g = chain_game("courier", vec![Target::Courier { a, b, r: 40.0, time_limit_min: 30.0 }]);
        g.on_estimate(&fixat(a, 10), None); // picked up
        g.set_counting(false, 0);
        g.set_counting(true, 0); // e.g. a ride in the car with Bluetooth connected
        let ev = g.on_estimate(&fixat(b, 300), None);
        assert_eq!(done_ids(&ev), vec![1000], "the pickup is kept: {ev:?}");
    }

    #[test]
    fn a_round_trips_far_point_survives_a_counting_pause() {
        let far = destination(home(), 0.0, 1500.0);
        let mut g = chain_game("round_trip", vec![Target::RoundTrip { far, r: 50.0 }]);
        g.on_estimate(&fixat(destination(home(), 0.0, 300.0), 0), None);
        g.on_estimate(&fixat(far, 200), None); // reached the far point
        g.set_counting(false, 0);
        g.set_counting(true, 0);
        let ev = g.on_estimate(&fixat(home(), 900), None);
        assert_eq!(done_ids(&ev), vec![1000], "the far point is kept: {ev:?}");
    }

    #[test]
    fn a_reach_quest_does_not_complete_while_counting_is_off() {
        let mut g = game(&reach_only(&[Mode::Walk], 10, "all_trips"), Backend::Solo, 4);
        let q = g.quest_views(0).remove(0);
        g.set_counting(false, 0);
        assert!(g.on_estimate(&fixat(q.anchor.unwrap(), 100), None).is_empty());
        assert!(!g.done.contains(&q.location_id));
        g.set_counting(true, 0);
        assert!(!g.on_estimate(&fixat(q.anchor.unwrap(), 200), None).is_empty());
    }

    #[test]
    fn a_completed_quest_is_logged_with_how_it_was_done_and_its_reward_with_where_it_came_from() {
        let mut g = game(&reach_only(&[Mode::Walk], 10, "all_trips"), Backend::Solo, 4);
        let q = g.quest_views(0).remove(0);
        let ev = g.on_estimate(&fixat(q.anchor.unwrap(), 100), None);
        let at = Some((q.anchor.unwrap().lat, q.anchor.unwrap().lon));
        let log = g.journal_events(&ev, 100_000, at);
        let done = log.iter().find(|e| e.kind == "quest_done").expect("a quest_done entry");
        assert!(done.detail.contains(&q.name) && done.detail.contains("Get within 40 m"), "{}", done.detail);
        let reward = log.iter().find(|e| e.kind == "reward").expect("a reward entry");
        assert!(reward.detail.contains(&format!("reward for {}", q.name)), "{}", reward.detail);
        assert!(reward.detail.len() > q.name.len() + 20, "carries an explanation of the item: {}", reward.detail);
        assert!(log.iter().all(|e| e.t_ms == 100_000 && e.at == at));
    }

    fn start_near_a_quest() -> (Game, i64, Point) {
        let mut g = game(&reach_only(&[Mode::Walk], 10, "all_trips"), Backend::Solo, 4);
        let q = g.quest_views(0).remove(0);
        let p0 = destination(q.anchor.unwrap(), 0.0, 200.0);
        g.on_fix(&raw(p0, 1000, 5.0), None);
        (g, q.location_id, p0)
    }

    fn target_of(g: &Game, id: i64) -> Point {
        g.assignments.iter().find(|a| a.location_id == id).and_then(|a| anchor(&a.target)).unwrap()
    }

    #[test]
    fn the_first_fix_after_resuming_is_not_judged_against_a_stale_one() {
        let (mut g, id, p0) = start_near_a_quest();
        g.set_counting(false, 0);
        g.set_counting(true, 0);
        // 200 m from the last fix 3 s later would be gated if the old filter were kept
        let ev = g.on_fix(&raw(target_of(&g, id), 1003, 5.0), None);
        assert!(ev.iter().any(|e| matches!(e, Event::QuestDone { location_id, .. } if *location_id == id)), "{ev:?} from {p0:?}");
    }

    #[test]
    fn the_trace_delta_of_another_game_is_a_reset() {
        // Task 19b: a game switch must never append to the old game's line.
        let walk = |g: &mut Game, p0: Point| {
            for i in 1..=20 {
                g.on_fix(&raw(destination(p0, 90.0, 1.4 * f64::from(i)), 1000 + i64::from(i), 5.0), None);
            }
        };
        let (mut a, _, pa) = start_near_a_quest();
        let (mut b, _, pb) = start_near_a_quest();
        walk(&mut a, pa);
        walk(&mut b, pb);
        let first = a.trace_matched_since(0);
        assert!(first.reset && !first.append.is_empty(), "{first:?}");
        assert!(!a.trace_matched_since(first.cursor).reset);
        assert!(b.trace_matched_since(first.cursor).reset, "another game's cursor");
    }

    #[test]
    fn distance_is_not_added_while_counting_is_off() {
        let (mut g, _, p0) = start_near_a_quest();
        let before = g.stats.distance_m;
        g.set_counting(false, 0);
        for i in 1..=10 {
            g.on_fix(&raw(destination(p0, 90.0, 30.0 * f64::from(i)), 1000 + i64::from(i) * 10, 5.0), None);
        }
        assert_eq!(g.stats.distance_m, before);
    }

    #[test]
    fn a_far_off_network_style_fix_is_dropped_even_on_the_target() {
        let (mut g, id, p0) = start_near_a_quest();
        let ev = g.on_fix(&raw(target_of(&g, id), 1003, 5.0), None);
        assert!(ev.is_empty() && !g.done.contains(&id), "a 200 m jump in 3 s is gated and must not complete the quest");
        assert_eq!(g.last_pos(), Some(p0), "the last good position is kept");
    }

    #[test]
    fn walking_to_the_target_still_completes_after_an_outlier() {
        let (mut g, id, _) = start_near_a_quest();
        let target = target_of(&g, id);
        g.on_fix(&raw(target, 1003, 5.0), None); // gated
        let ev = g.on_fix(&raw(target, 1100, 5.0), None); // 200 m in 100 s: a brisk walk
        assert!(ev.iter().any(|e| matches!(e, Event::QuestDone { location_id, .. } if *location_id == id)));
    }

    #[test]
    fn several_far_fixes_in_a_row_are_believed() {
        let (mut g, _, p0) = start_near_a_quest();
        let far = destination(p0, 90.0, 5000.0);
        for (i, t) in [1003, 1006, 1009].into_iter().enumerate() {
            g.on_fix(&raw(far, t, 5.0), None);
            assert_eq!(g.last_pos() == Some(far), i == 2, "believed only on the third in a row (step {i})");
        }
    }

    #[test]
    fn fixes_worse_than_35_m_are_ignored() {
        let (mut g, id, _) = start_near_a_quest();
        let ev = g.on_fix(&raw(target_of(&g, id), 2000, 40.0), None);
        assert!(ev.is_empty() && !g.done.contains(&id));
        // 1000 s after the last fix this is a restart; over 35 m it keeps its verdict and does not count (ruling FR-I1).
        assert!(g.last_estimate().is_some_and(|e| e.verdict == LocVerdict::Reset && !e.accepted && e.uncertain()), "{:?}", g.last_estimate());
    }

    #[test]
    fn standing_still_with_gps_jitter_adds_no_distance() {
        let (mut g, _, p0) = start_near_a_quest();
        for i in 0..60 {
            let wobble = destination(p0, (i * 97 % 360) as f64, 3.0 + (i % 3) as f64);
            g.on_fix(&raw(wobble, 1005 + i * 5, 5.0), None);
        }
        assert!(g.stats.distance_m < 12.0, "jitter counted as {} m", g.stats.distance_m);
    }

    #[test]
    fn a_long_gap_is_not_walked_distance() {
        let (mut g, _, p0) = start_near_a_quest();
        g.on_fix(&raw(destination(p0, 90.0, 20_000.0), 1000 + 3600, 5.0), None);
        assert!(g.stats.distance_m < 1.0, "an hour later 20 km away is a relocation, counted {}", g.stats.distance_m);
    }

    #[test]
    fn walking_adds_about_the_distance_walked() {
        let (mut g, _, p0) = start_near_a_quest();
        for i in 1..=40 {
            g.on_fix(&raw(destination(p0, 90.0, 7.0 * i as f64), 1000 + i * 5, 5.0), None);
        }
        let d = g.stats.distance_m;
        assert!((d - 280.0).abs() < 28.0, "walked 280 m, counted {d}");
    }

    #[test]
    fn near_a_quest_the_reason_it_does_or_does_not_count_is_explained() {
        let (g, id, p0) = start_near_a_quest();
        let target = target_of(&g, id);
        let near = |e: Estimate| g.explain_near(&e, 100.0).into_iter().find(|n| n.location_id == id);
        assert!(near(fixat(p0, 1000)).is_none(), "200 m away is not near");
        let nm = near(fixat(destination(target, 0.0, 70.0), 1000)).expect("70 m away is near");
        assert!((nm.distance_m - 70.0).abs() < 2.0 && nm.reason.contains("needs 40"), "{nm:?}");
        // The estimate's verdict decides the reason.
        let gated = Estimate { verdict: LocVerdict::Gated, accepted: false, ..fixat(target, 1003) };
        assert!(near(gated).unwrap().reason.contains("jump"));
        let blurry = Estimate { verdict: LocVerdict::Blurry, accepted: false, uncertainty_m: 60.0, ..fixat(target, 2000) };
        assert!(near(blurry).unwrap().reason.contains("GPS uncertain (60 m, needs 35 m)"));
        let bridged = Estimate { source: Source::Bridged, accepted: false, ..fixat(target, 2001) };
        assert!(near(bridged).unwrap().reason.contains("estimated from steps"));
    }

    #[test]
    fn poor_accuracy_fixes_are_ignored() {
        let mut g = game(&reach_only(&[Mode::Walk], 10, "all_trips"), Backend::Solo, 4);
        let q = g.quest_views(0).remove(0);
        assert!(g.on_fix(&raw(q.anchor.unwrap(), 5, 300.0), None).is_empty());
        assert!(g.last_estimate().is_none(), "an unusable first fix places nobody");
    }

    #[test]
    fn the_game_shows_the_locators_position_and_passes_compass_readings_on() {
        let mut g = game(&reach_only(&[Mode::Walk], 10, "all_trips"), Backend::Solo, 4);
        assert!(g.position(0).is_none(), "nothing before the first fix");
        let a = g.quest_views(0).remove(0).anchor.unwrap();
        g.on_fix(&raw(a, 1, 5.0), None);
        let d = g.position(1000).unwrap();
        assert!(distance_m(Point::new(d.lat, d.lon), a) < 1.0 && d.snap);
        let az = |t_ms| HeadingIn { t_ms, azimuth_deg: 120.0, accuracy: crate::loc::CompassAccuracy::High, ..Default::default() };
        (0..4).for_each(|i| g.on_heading(&az(500 + i * 500)));
        assert_eq!(g.position(2000).unwrap().heading_source, crate::loc::HeadingSource::Compass);
    }

    #[test]
    fn an_unusable_first_fix_is_not_the_last_estimate_but_a_later_one_keeps_the_last_place() {
        // Controller note 2 (Task 10 minor 6): the engine takes the travel mode from the last estimate; a 1000 m, mock or NaN first fix
        // must not decide it.
        let mut g = game(&reach_only(&[Mode::Walk], 10, "all_trips"), Backend::Solo, 4);
        let a = g.quest_views(0).remove(0).anchor.unwrap();
        let far = destination(a, 0.0, 50_000.0);
        g.on_fix(&raw(far, 1, 1000.0), None);
        g.on_fix(&RawFix { mock: true, ..raw(far, 2, 5.0) }, None);
        g.on_fix(&RawFix { lat: f64::NAN, ..raw(far, 3, 5.0) }, None);
        assert!(g.last_estimate().is_none());
        g.on_fix(&raw(a, 4, 5.0), None);
        g.on_fix(&raw(far, 5, 1000.0), None);
        let e = g.last_estimate().unwrap();
        assert_eq!(e.verdict, LocVerdict::Unusable);
        assert!(distance_m(e.point(), a) < 1.0, "an unusable fix after a good one keeps the good place");
    }

    #[test]
    fn an_unusable_first_fix_places_nobody_and_a_usable_one_is_placed_by_its_own_estimate() {
        // Review M10, ruling E2: a fix that placed nobody says nothing about the zone; a usable one hands over its own estimate.
        let mut g = game(&reach_only(&[Mode::Walk], 10, "all_trips"), Backend::Solo, 4);
        let a = g.quest_views(0).remove(0).anchor.unwrap();
        let mut placed = None;
        g.on_fix_with_zone(&raw(a, 1, 1000.0), None, |p| placed = Some(p));
        assert!(placed.is_none() && g.last_estimate().is_none());
        g.on_fix_with_zone(&raw(a, 2, 5.0), None, |p| placed = Some(p));
        assert!(placed.is_some_and(|p| distance_m(p, a) < 1.0), "a usable fix is placed: {placed:?}");
    }

    #[test]
    fn walking_quests_do_not_count_while_moving_at_vehicle_speed() {
        let mut g = game(&reach_only(&[Mode::Walk], 10, "all_trips"), Backend::Solo, 4);
        let q = g.quest_views(0).remove(0);
        let ev = g.on_estimate(&Estimate { speed_mps: 100.0, ..fixat(q.anchor.unwrap(), 10) }, None); // 360 km/h
        assert!(!ev.iter().any(|e| matches!(e, Event::QuestDone { .. })));
    }

    #[test]
    fn a_gated_fix_never_completes_a_quest() {
        let (mut g, id, _) = start_near_a_quest();
        let target = g.assignments.iter().find(|a| a.location_id == id).and_then(|a| anchor(&a.target)).unwrap();
        let ev = g.on_fix(&raw(target, 1003, 5.0), None);
        assert_eq!(g.last_estimate().map(|e| e.verdict), Some(LocVerdict::Gated));
        assert!(ev.is_empty() && !g.done.contains(&id));
    }

    #[test]
    fn a_filter_reset_pauses_a_dwell_and_a_full_dwell_after_it_still_counts() {
        let mut g = chain_game("dwell", vec![Target::Dwell { p: home(), r: 50.0, minutes: 10.0 }]);
        for t in (0..=300).step_by(5) {
            g.on_fix(&raw(home(), t, 5.0), None);
        }
        let ev = g.on_fix(&raw(home(), 700, 5.0), None);
        assert_eq!(g.last_estimate().map(|e| e.verdict), Some(LocVerdict::Reset), "400 s without a fix");
        assert!(ev.is_empty() && g.done.is_empty(), "the timer starts again after a reset");
        let mut done = Vec::new();
        for t in (705..=1320).step_by(5) {
            done.extend(done_ids(&g.on_fix(&raw(home(), t, 5.0), None)));
        }
        assert_eq!(done, vec![1000]);
    }

    #[test]
    fn a_trap_in_a_pause_is_placed_at_the_last_accepted_position_before_it() {
        // Adversarial re-review N5: a counting pause (home, car) forgot the accepted position, so a trap arriving then was placed
        // around the game's home. The last accepted position outlives the pause; home is the fallback only before the first one.
        let mut g = chain_game("dwell", vec![Target::Dwell { p: home(), r: 50.0, minutes: 10.0 }]);
        assert_eq!(g.last_accepted_pos(), None, "nothing accepted yet");
        let there = destination(home(), 0.0, 2000.0);
        g.on_fix(&raw(there, 1, 5.0), None);
        g.set_counting(false, 0);
        assert_eq!(g.last_pos(), None, "checks forget it across the pause");
        assert!(g.last_accepted_pos().is_some_and(|p| distance_m(p, there) < 1e-6), "{:?}", g.last_accepted_pos());
        g.set_counting(true, 0);
        assert!(g.last_accepted_pos().is_some_and(|p| distance_m(p, there) < 1e-6), "and after it, until the next accepted fix");
    }

    #[test]
    fn the_time_in_a_bridged_gap_is_not_dwelt() {
        // Adversarial re-review (out of scope, quest integrity): a bridged gap ended with a `Used` fix, so the first accepted fix after it
        // credited the whole gap to a dwell started before it. It pauses the trackers, as a restart does.
        let mut g = chain_game("dwell", vec![Target::Dwell { p: home(), r: 1000.0, minutes: 10.0 }]);
        let at = |t: i64| destination(home(), 90.0, 1.4 * i64_to_f64(t));
        let mut bridged = false;
        let mut done_at = None;
        for t in 0..=700 {
            if t % 2 == 0 {
                g.on_steps(10_000 + t * 193 / 100, t * 1000, None);
            }
            g.on_heading(&HeadingIn {
                t_ms: t * 1000,
                azimuth_deg: 90.0,
                accuracy: crate::loc::CompassAccuracy::High,
                pitch_deg: 10.0,
                ..HeadingIn::default()
            });
            bridged |= g.position(t * 1000).is_some_and(|d| d.source == crate::loc::DisplaySource::Bridged);
            if !(300..400).contains(&t) && !done_ids(&g.on_fix(&raw(at(t), t, 4.0), None)).is_empty() {
                done_at = done_at.or(Some(t));
            }
        }
        assert!(bridged, "the gap 300..400 s was bridged");
        assert_eq!(done_at, None, "a 10 min dwell done at {done_at:?} s although 100 s of it were a bridged gap");
    }

    #[test]
    fn step_events_reach_the_filter_at_their_own_time_even_after_a_fix() {
        // Ruling FR-I3 (final review I3): the phone delivers step events up to 2 s late. A fix in between (carrying the total already
        // delivered) must not stamp that total at the fix time and so drop the late event: every event is read at its own time.
        let (mut g, _, p0) = start_near_a_quest();
        let mut events = Vec::new();
        let mut delivered = None;
        for k in 0..30_i64 {
            let t_s = 1000 + k;
            if k % 2 == 1 {
                let (t_e, n) = ((t_s - 2) * 1000 + 800, 20_000 + 2 * k); // 1.2 s late: older than the last fix
                g.on_steps(n, t_e, None);
                events.push((t_e, n));
                delivered = Some(n);
            }
            g.on_fix(&raw(destination(p0, 90.0, 1.4 * i64_to_f64(k)), t_s, 5.0), delivered);
        }
        let h = g.locator.steps();
        assert!(events.iter().all(|(t, n)| h.total_at(*t) == Some(*n)), "{h:?}");
        assert_eq!(h.latest(), events.last().copied());
    }

    #[test]
    fn a_fix_time_step_total_is_only_a_fallback_and_never_hides_a_later_event() {
        // Ruling FR-I3: without a step event for 10 s the total sent with a fix is kept at the fix time; a step event then delivered
        // for an earlier time replaces it instead of being dropped.
        let (mut g, _, p0) = start_near_a_quest();
        g.on_steps(500, 1_000_000, None);
        g.on_fix(&raw(p0, 1005, 5.0), Some(500));
        assert_eq!(g.locator.steps().latest(), Some((1_000_000, 500)), "an event 5 s ago: no fallback");
        g.on_fix(&raw(p0, 1020, 5.0), Some(500));
        assert_eq!(g.locator.steps().latest(), Some((1_020_000, 500)), "20 s without an event: the fix-time total");
        g.on_steps(530, 1_019_500, None);
        assert_eq!(g.locator.steps().latest(), Some((1_019_500, 530)), "the late event wins");
    }

    #[test]
    fn a_coarse_filter_reset_still_pauses_a_dwell() {
        // Ruling FR-I1: a restart over 35 m is not accepted but is still a restart. The dwell timer pauses, so the 400 s without a fix
        // do not count as dwelt; the full 10 min after the reset still do.
        let mut g = chain_game("dwell", vec![Target::Dwell { p: home(), r: 50.0, minutes: 10.0 }]);
        for t in (0..=300).step_by(5) {
            g.on_fix(&raw(home(), t, 5.0), None);
        }
        g.on_fix(&raw(home(), 700, 45.0), None);
        let e = g.last_estimate().unwrap();
        assert!(e.verdict == LocVerdict::Reset && !e.accepted, "{e:?}");
        let near = g.explain_near(&e, 100.0);
        assert!(!near.is_empty() && near.iter().all(|n| n.reason.starts_with("GPS uncertain")), "{near:?}");
        let mut done = Vec::new();
        for t in (705..=1320).step_by(5) {
            if !done_ids(&g.on_fix(&raw(home(), t, 5.0), None)).is_empty() {
                done.push(t);
            }
        }
        assert!(done.first().is_some_and(|t| *t >= 1300), "dwell done at {done:?}");
    }

    #[test]
    fn create_and_load_pick_the_fastest_playable_mode_and_a_travel_mode_sets_filter_and_odometer() {
        let dir = std::env::temp_dir().join(format!("apgo-game-mode-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut g = game(&reach_only(&[Mode::Run], 12, "all_trips"), Backend::Solo, 1);
        assert_eq!(g.locator.mode(), Mode::Walk, "created: run quests are locked, so walking rules");
        g.receive_item("Running Shoes", 0, None);
        assert_eq!((g.locator.mode(), g.odometer.speed_cap_mps()), (Mode::Run, mode_cap_mps(Mode::Run)), "the shoes make runs playable");
        g.save(&dir).unwrap();
        let back = Game::load(&dir, &g.id).unwrap();
        assert_eq!((back.locator.mode(), back.odometer.speed_cap_mps()), (Mode::Run, mode_cap_mps(Mode::Run)), "loaded: fastest playable mode");
        g.sync_items(&[], 1, None);
        assert_eq!(g.locator.mode(), Mode::Walk, "the server took the shoes back");
        g.set_travel_mode(Mode::Run);
        g.set_travel_mode(Mode::Walk);
        assert_eq!((g.locator.mode(), g.odometer.speed_cap_mps()), (Mode::Walk, mode_cap_mps(Mode::Walk)));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_skipped_odometer_never_has_a_zero_speed_cap() {
        let g: Game = serde_json::from_value(serde_json::to_value(game(&reach_only(&[Mode::Walk], 10, "all_trips"), Backend::Solo, 4)).unwrap()).unwrap();
        assert_eq!(g.odometer.speed_cap_mps(), mode_cap_mps(Mode::Walk), "serde's default for the skipped field is the Walk cap");
    }

    #[test]
    fn the_last_estimate_is_kept_while_counting_is_off() {
        let mut g = game(&reach_only(&[Mode::Walk], 10, "all_trips"), Backend::Solo, 4);
        g.set_counting(false, 0);
        g.on_fix(&raw(home(), 1, 5.0), None);
        assert!(g.last_estimate().is_some(), "the engine places the player from it even at home");
    }

    #[test]
    fn a_counting_toggle_restarts_the_filter() {
        let (mut g, _, p0) = start_near_a_quest();
        g.set_counting(false, 0);
        g.set_counting(true, 0);
        g.on_fix(&raw(destination(p0, 90.0, 3000.0), 1003, 5.0), None);
        assert_eq!(g.last_estimate().map(|e| e.verdict), Some(LocVerdict::Reset), "a fresh filter believes the first fix after a pause");
    }

    #[test]
    fn the_filter_is_not_saved_and_a_save_loads_without_it() {
        let (g, _, _) = start_near_a_quest();
        let v = serde_json::to_value(&g).unwrap();
        assert!(v.get("locator").is_none() && v.get("odometer").is_none() && v.get("last_est").is_none());
        let back: Game = serde_json::from_value(v).unwrap();
        assert!(back.last_estimate().is_none() && back.last_pos().is_none());
    }

    #[test]
    fn a_simulated_fix_teleports_and_completes_like_today() {
        let mut g = game(&reach_only(&[Mode::Walk], 10, "all_trips"), Backend::Solo, 4);
        let q = g.quest_views(0).remove(0);
        g.on_fix(&RawFix { provider: Provider::Sim, ..raw(destination(q.anchor.unwrap(), 0.0, 5000.0), 1, 5.0) }, None);
        let ev = g.on_fix(&RawFix { provider: Provider::Sim, ..raw(q.anchor.unwrap(), 2, 5.0) }, None);
        assert!(done_ids(&ev).contains(&q.location_id), "5 km in 1 s is fine for the simulator: {ev:?}");
    }

    #[test]
    fn a_gap_reset_in_a_walk_zone_counts_its_chord() {
        // Ruling T8-R13 through the game: the filter restarts after a GPS gap (lost) and the walkable chord is travelled distance.
        let (mut g, _, p0) = start_near_a_quest();
        for i in 1..=12 {
            g.on_fix(&raw(destination(p0, 90.0, 7.0 * i as f64), 1000 + i * 5, 5.0), None);
        }
        let before = g.stats.distance_m;
        g.on_fix(&raw(destination(p0, 90.0, 84.0 + 200.0), 1060 + 200, 5.0), None); // 200 m in 200 s
        assert_eq!(g.last_estimate().map(|e| e.verdict), Some(LocVerdict::Reset));
        let added = g.stats.distance_m - before;
        assert!((added - 200.0).abs() < 5.0, "the chord from the last estimate counts: {added}");
    }

    #[test]
    fn a_relocation_at_vehicle_speed_out_of_a_hold_adds_no_distance_in_a_walk_zone() {
        // Rulings T8-R18, R20 through the game: the chord of a relocation that ends a hold is judged by the agreeing fixes' own speed.
        let (mut g, _, p0) = start_near_a_quest();
        for t in 1001..=1040 {
            g.on_fix(&raw(destination(p0, f64::from(u16::try_from(t * 97 % 360).unwrap()), 2.0), t, 6.0), Some(100));
        }
        let mut relocated = Vec::new();
        for k in 1..=10 {
            let before = g.stats.distance_m;
            g.on_fix(&raw(destination(p0, 90.0, 20.0 * k as f64), 1040 + k, 6.0), Some(100)); // 20 m/s
            if g.last_estimate().map(|e| e.verdict) == Some(LocVerdict::Relocated) {
                relocated.push(g.stats.distance_m - before);
            }
        }
        assert!(!relocated.is_empty() && relocated.iter().all(|d| *d == 0.0), "20 m/s is no walk: {relocated:?}");
    }

    #[test]
    fn a_saved_game_from_before_chains_loads_with_defaults() {
        let g = game(&reach_only(&[Mode::Walk], 10, "all_trips"), Backend::Solo, 4);
        let mut v = serde_json::to_value(&g).unwrap();
        v.as_object_mut().unwrap().remove("counters");
        v.as_object_mut().unwrap().insert("away".into(), serde_json::json!({"zone_only": true, "distance_m": {"1": 900.0}}));
        let back: Game = serde_json::from_value(v).unwrap();
        assert!(back.counters.progress.is_empty() && back.counters.away_mark.is_none(), "an old save's away settings are ignored");
    }

    #[test]
    fn a_saved_game_from_before_the_parking_switch_allows_parking_lots() {
        let mut g = game(&reach_only(&[Mode::Walk], 10, "all_trips"), Backend::Solo, 4);
        assert!(g.allow_parking, "a new game allows parking lots by default");
        g.allow_parking = false;
        let mut v = serde_json::to_value(&g).unwrap();
        assert!(!serde_json::from_value::<Game>(v.clone()).unwrap().allow_parking, "the choice is saved");
        v.as_object_mut().unwrap().remove("allow_parking");
        assert!(serde_json::from_value::<Game>(v).unwrap().allow_parking, "an old save keeps its quests where they were allowed");
    }

    /// A solo game whose only quest (2000) is a forager: 6 acorns 100 m apart going north from 500 m, need 3.
    fn forager_game() -> (Game, Vec<Point>) {
        let mut g = game(&reach_only(&[Mode::Walk], 10, "all_trips"), Backend::Solo, 4);
        let pts: Vec<Point> = (0..6u32).map(|i| destination(home(), 0.0, 500.0 + 100.0 * f64::from(i))).collect();
        let mut a = chain::tests_support::member(2000, 1, "forager", Target::Collect { pts: pts.clone(), need: 3, r: 25.0, theme: "acorns".into() });
        a.family = "courier".into();
        g.assignments = vec![a];
        g.done.clear();
        g.solo_rewards = BTreeMap::from([(2000, "Hydrate!".to_string())]);
        (g, pts)
    }

    fn carried_banked(g: &Game) -> (u32, u32) {
        g.collected.get(&2000).map_or((0, 0), |c| (c.carried, c.banked))
    }

    fn live(g: &Game) -> Vec<Point> {
        g.quest_views(0).into_iter().find(|v| v.location_id == 2000).unwrap().live_items
    }

    /// A step chain (one mark at 5000) and a forager carrying `carried` items, with the step baseline set.
    fn loaded_walker(carried: u32) -> Game {
        let mut g = chain_game("step_up", vec![Target::Steps { n: 5000 }]);
        let forager = Target::Collect { pts: vec![destination(home(), 0.0, 500.0)], need: 3, r: 25.0, theme: "acorns".into() };
        g.assignments.push(chain::tests_support::member(2000, 1, "forager", forager));
        g.collected.insert(2000, Collected { carried, ..Collected::default() });
        g.on_steps(10_000, 1, None);
        g
    }

    fn steps_counted(g: &Game) -> f64 {
        g.counters.progress.values().sum()
    }

    #[test]
    fn overloaded_steps_count_less() {
        let mut g = loaded_walker(22); // bag 20: 2 over
        assert!((g.step_factor() - 0.9).abs() < 1e-12);
        g.on_steps(11_000, 2, None);
        assert!((steps_counted(&g) - 900.0).abs() < 1e-9, "{}", steps_counted(&g));
        let mut light = loaded_walker(20);
        light.on_steps(11_000, 2, None);
        assert!((steps_counted(&light) - 1000.0).abs() < 1e-9, "a full bag is not over");
    }

    #[test]
    fn banking_lifts_the_step_penalty() {
        let mut g = loaded_walker(40);
        g.set_counting(false, 0);
        g.bank_at_home(5);
        g.set_counting(true, 0);
        g.on_steps(10_000, 6, None); // counting resumes from here
        g.on_steps(11_000, 7, None);
        assert!((g.step_factor() - 1.0).abs() < 1e-12);
        assert!((steps_counted(&g) - 1000.0).abs() < 1e-9, "{}", steps_counted(&g));
    }

    #[test]
    fn a_done_forager_carries_nothing_into_the_bag() {
        let mut g = loaded_walker(30);
        g.done.insert(2000);
        assert_eq!(g.carried_total(), 0);
    }

    #[test]
    fn picking_on_a_fix_removes_live_items_and_carries_them() {
        let (mut g, pts) = forager_game();
        assert_eq!(live(&g), pts, "before any pool: the placed items");
        g.on_estimate(&fixat(pts[0], 600), None);
        assert_eq!((live(&g), carried_banked(&g)), (pts[1..].to_vec(), (1, 0)));
        assert_eq!(g.collected[&2000].picked, BTreeSet::from([0]), "a placed item is still recorded by index (the shuffle trap keeps it)");
    }

    #[test]
    fn old_saves_seed_live_items_from_the_unpicked_points() {
        let (mut g, pts) = forager_game();
        g.collected.insert(2000, Collected { picked: BTreeSet::from([0, 2]), carried: 1, banked: 1 });
        let mut old = serde_json::to_value(&g).unwrap();
        old.as_object_mut().unwrap().remove("forage");
        old.as_object_mut().unwrap().remove("bag_size");
        let back: Game = serde_json::from_value(old).unwrap();
        assert_eq!(live(&back), vec![pts[1], pts[3], pts[4], pts[5]]);
        assert_eq!((back.bag_size, back.collected[&2000].carried, back.collected[&2000].banked), (20, 1, 1));
    }

    #[test]
    fn live_items_come_nearest_home_first() {
        // Found on the phone: after a top-up the first item (where a card opened from the list sits) was any of them, often the
        // farthest; nearest home first keeps it the next one to fetch.
        let (mut g, _) = forager_game();
        g.set_forage_pools(&[realm("r0", Mode::Walk)]);
        let d: Vec<f64> = live(&g).iter().map(|p| distance_m(home(), *p)).collect();
        assert!(d.windows(2).all(|w| w[0] <= w[1]), "{d:?}");
    }

    #[test]
    fn the_first_pools_fill_a_forager_to_three_times_its_need_once() {
        let (mut g, _) = forager_game();
        g.set_forage_pools(&[realm("r0", Mode::Walk)]);
        assert_eq!(live(&g).len(), 9, "3 x need 3");
        let first = live(&g)[0];
        g.on_estimate(&fixat(first, 600), None);
        g.set_forage_pools(&[realm("r0", Mode::Walk)]);
        assert_eq!(live(&g).len(), 8, "a second build (app opened again) does not skip the wait");
    }

    #[test]
    fn a_due_top_up_waits_for_the_pools() {
        let (mut g, _) = forager_game();
        g.set_forage_pools(&[realm("r0", Mode::Walk)]);
        let first = live(&g)[0];
        g.on_estimate(&fixat(first, 600), None);
        let due = 600_000 + RESPAWN_MS; // fixat takes seconds
        g.tick(due - 1);
        assert_eq!(live(&g).len(), 8, "not yet");
        g.forage_pools.clear(); // reopened: the streets build has not run yet
        g.tick(due);
        assert_eq!(live(&g).len(), 8, "no pool: waits");
        assert_eq!(g.next_due_ms(due), None, "and asks for no wake-up it cannot use");
        g.set_forage_pools(&[realm("r0", Mode::Walk)]);
        g.tick(due + 1);
        assert_eq!(live(&g).len(), 9);
    }

    #[test]
    fn next_due_ms_includes_the_respawn() {
        let (mut g, _) = forager_game();
        g.set_forage_pools(&[realm("r0", Mode::Walk)]);
        assert_eq!(g.next_due_ms(0), None, "full: nothing due");
        let first = live(&g)[0];
        g.on_estimate(&fixat(first, 600), None);
        assert_eq!(g.next_due_ms(600_000), Some(600_000 + RESPAWN_MS));
    }

    #[test]
    fn a_shuffle_trap_keeps_the_respawn_clock_and_refills_from_the_pool() {
        let (mut g, _) = forager_game();
        let realms = vec![realm("r0", Mode::Walk)];
        g.set_forage_pools(&realms);
        let first = live(&g)[0];
        g.on_estimate(&fixat(first, 600), None);
        let due = g.forage[&2000].next_spawn_ms;
        assert_eq!(g.reroll(&[2000], &realms, 9, &Catalog::builtin()).unwrap(), 1);
        assert_eq!(live(&g).len(), 9, "with a pool, the moved forager is full again at once");
        // Without a pool (the build has not run): the clock survives, so the top-up still comes when it was due.
        let (mut g, _) = forager_game();
        g.set_forage_pools(&realms);
        let first = live(&g)[0];
        g.on_estimate(&fixat(first, 600), None);
        g.forage_pools.clear();
        g.reroll(&[2000], &realms, 9, &Catalog::builtin()).unwrap();
        assert_eq!(g.forage.get(&2000).and_then(|f| f.next_spawn_ms), due);
    }

    #[test]
    fn a_done_forager_clears_its_items_and_never_respawns() {
        let (mut g, pts) = forager_game();
        for (i, p) in pts.iter().take(3).enumerate() {
            g.on_estimate(&fixat(*p, 600 * (i64::try_from(i).unwrap() + 1)), None);
        }
        g.set_forage_pools(&[realm("r0", Mode::Walk)]);
        assert_eq!(done_ids(&g.on_estimate(&fixat(home(), 3000), None)), vec![2000]);
        assert!(live(&g).is_empty());
        assert_eq!(g.next_due_ms(3000), None);
        g.tick(3000 + RESPAWN_MS);
        assert!(live(&g).is_empty(), "never respawns");
    }

    /// The forager with its first item picked at t = 0 (5 left of 9 wanted, a top-up due at `RESPAWN_MS`) and `pool` to top up from.
    fn due_forager(pool: Vec<Point>) -> (Game, Vec<Point>) {
        let (mut g, pts) = forager_game();
        g.on_estimate(&fixat(pts[0], 0), None);
        g.forage_pools.insert(2000, pool);
        (g, pts)
    }

    #[test]
    fn a_due_top_up_runs_on_a_fix_and_on_steps() {
        let free = destination(home(), 90.0, 700.0);
        let (mut g, _) = due_forager(vec![free]);
        g.on_estimate(&fixat(home(), RESPAWN_MS / 1000), None);
        assert!(live(&g).contains(&free), "on a fix");
        let (mut g, _) = due_forager(vec![free]);
        g.on_steps(100, RESPAWN_MS, None);
        assert!(live(&g).contains(&free), "on steps");
    }

    #[test]
    fn a_short_pool_tries_again_a_respawn_later() {
        let first = destination(home(), 90.0, 700.0);
        let (mut g, _) = due_forager(vec![first]);
        g.tick(RESPAWN_MS);
        assert_eq!((live(&g).len(), g.forage[&2000].next_spawn_ms), (6, Some(2 * RESPAWN_MS)), "5 left and the pool's 1, short of 9");
        let more = (1..=3).map(|i| destination(home(), 90.0, 700.0 + 100.0 * f64::from(i)));
        g.forage_pools.insert(2000, std::iter::once(first).chain(more).collect());
        g.tick(2 * RESPAWN_MS - 1);
        assert_eq!(live(&g).len(), 6, "not before its time");
        g.tick(2 * RESPAWN_MS);
        assert_eq!((live(&g).len(), g.forage[&2000].next_spawn_ms), (9, None));
    }

    #[test]
    fn one_fix_picks_every_item_in_reach() {
        let (mut g, pts) = forager_game();
        let f = g.forage_entry(2000).unwrap();
        f.items.push(crate::forage::Item { id: f.next_id, p: destination(pts[0], 90.0, 20.0) });
        f.next_id += 1;
        g.on_estimate(&fixat(destination(pts[0], 90.0, 10.0), 600), None);
        assert_eq!((carried_banked(&g), g.collected[&2000].picked.clone()), ((2, 0), BTreeSet::from([0])), "a topped-up item has no placed index");
        assert_eq!(live(&g), pts[1..].to_vec());
    }

    #[test]
    fn a_top_up_never_places_an_item_where_the_player_stands() {
        let here = destination(home(), 90.0, 700.0);
        let (mut g, _) = due_forager(vec![here]);
        g.on_estimate(&fixat(here, RESPAWN_MS / 1000), None);
        assert_eq!((live(&g).contains(&here), carried_banked(&g)), (false, (1, 0)), "not placed and picked on the same fix");
        let (mut g, _) = due_forager(vec![here]);
        g.on_estimate(&fixat(here, 60), None);
        g.tick(RESPAWN_MS);
        assert!(!live(&g).contains(&here), "nor on a wake-up, where the last fix put the player");
    }

    #[test]
    fn a_top_up_keeps_clear_of_other_quests_and_other_foragers_items() {
        let (q, o, free) = (destination(home(), 90.0, 700.0), destination(home(), 180.0, 700.0), destination(home(), 270.0, 700.0));
        let (mut g, _) = due_forager(vec![destination(q, 0.0, 20.0), destination(o, 0.0, 20.0), free]);
        g.assignments.push(chain::tests_support::member(2001, 1, "landmark", Target::Point { p: q, r: 40.0 }));
        let mut other = chain::tests_support::member(2002, 1, "forager", Target::Collect { pts: vec![o], need: 1, r: 25.0, theme: "shells".into() });
        other.family = "courier".into();
        g.assignments.push(other);
        g.tick(RESPAWN_MS);
        assert_eq!((live(&g).len(), live(&g).contains(&free)), (6, true), "only the free spot");
    }

    #[test]
    fn a_restarted_forager_tracker_gets_no_placed_points() {
        let (mut g, _) = forager_game();
        g.on_estimate(&fixat(home(), 60), None); // makes its tracker
        let mut back: Game = serde_json::from_value(serde_json::to_value(&g).unwrap()).unwrap();
        back.reattach_trackers();
        assert!(matches!(back.trackers[&2000].target(), Target::Collect { pts, .. } if pts.is_empty()), "the game picks items, never the tracker");
    }

    #[test]
    fn a_refill_after_a_shuffle_trap_draws_other_spots_than_the_first_fill() {
        let (mut g, _) = forager_game();
        g.forage_pools.insert(2000, (0..20).map(|i| destination(home(), 18.0 * f64::from(i), 1500.0)).collect());
        let topped = |g: &Game| -> Vec<Point> { g.forage[&2000].items.iter().filter(|i| i.id >= 6).map(|i| i.p).collect() };
        g.fill_unfilled();
        let first = topped(&g);
        g.forage.remove(&2000); // as a Shuffle trap leaves it: seeded again with its seed, not filled
        g.forage_entry(2000).unwrap().salt = 9;
        g.fill_unfilled();
        assert_ne!(topped(&g), first, "a refill with the first fill's seed brings back the same (maybe picked) spots");
    }

    #[test]
    fn a_shuffle_trap_before_the_streets_build_keeps_its_seed_for_the_later_fill() {
        let (mut g, _) = forager_game();
        g.reroll(&[2000], &[realm("r0", Mode::Walk)], 9, &Catalog::builtin()).unwrap(); // no pools yet: nothing filled
        assert_eq!((g.forage[&2000].filled, g.forage[&2000].salt), (false, 9));
    }

    #[test]
    fn a_top_up_keeps_clear_of_a_couriers_drop_off_and_a_trails_whole_route() {
        let (b, mid, free) = (destination(home(), 90.0, 700.0), destination(home(), 180.0, 700.0), destination(home(), 270.0, 700.0));
        let (mut g, _) = due_forager(vec![destination(b, 0.0, 20.0), destination(mid, 0.0, 20.0), free]);
        let courier = Target::Courier { a: destination(home(), 45.0, 2000.0), b, r: 40.0, time_limit_min: 30.0 };
        g.assignments.push(chain::tests_support::member(2001, 1, "courier", courier));
        let trail = Target::Line { pts: vec![destination(home(), 225.0, 2000.0), mid], corridor_m: 20.0, coverage: 0.8 };
        g.assignments.push(chain::tests_support::member(2002, 1, "trail", trail));
        g.tick(RESPAWN_MS);
        assert_eq!((live(&g).len(), live(&g).contains(&free)), (6, true), "only the free spot");
    }

    #[test]
    fn a_top_up_keeps_clear_of_a_trail_between_its_corners() {
        let (start, end) = (destination(home(), 180.0, 700.0), destination(home(), 180.0, 1700.0));
        let on_the_way = destination(destination(home(), 180.0, 1200.0), 90.0, 20.0);
        let free = destination(home(), 270.0, 700.0);
        let (mut g, _) = due_forager(vec![on_the_way, free]);
        let trail = Target::Line { pts: vec![start, end], corridor_m: 20.0, coverage: 0.8 };
        g.assignments.push(chain::tests_support::member(2002, 1, "trail", trail));
        g.tick(RESPAWN_MS);
        assert!(!live(&g).contains(&on_the_way), "an item 20 m off the trail, 500 m from either corner");
        assert!(live(&g).contains(&free));
    }

    #[test]
    fn a_top_up_keeps_out_of_pick_range_even_past_the_item_spacing() {
        let here = destination(home(), 90.0, 700.0);
        let (mut g, _) = due_forager(vec![destination(here, 0.0, 70.0)]);
        if let Target::Collect { r, .. } = &mut g.assignments[0].target {
            *r = 80.0;
        }
        g.on_estimate(&fixat(here, RESPAWN_MS / 1000), None);
        assert_eq!(carried_banked(&g), (1, 0), "not placed and picked on the same fix");
    }

    #[test]
    fn a_rejected_estimate_is_not_where_a_top_up_keeps_clear_of() {
        let here = destination(home(), 90.0, 700.0);
        let (mut g, _) = due_forager(vec![destination(here, 0.0, 30.0)]);
        g.on_estimate(&fixat(here, 60), None);
        g.on_estimate(&Estimate { accepted: false, ..fixat(destination(home(), 270.0, 700.0), RESPAWN_MS / 1000) }, None);
        assert!(!live(&g).contains(&destination(here, 0.0, 30.0)), "the player was last seen 30 m from it");
    }

    #[test]
    fn a_top_up_in_a_bridged_gap_keeps_clear_of_where_the_steps_put_the_player() {
        let (mut g, id, p0) = start_near_a_quest();
        let course = crate::geo::bearing_deg(p0, target_of(&g, id));
        let at = |s: i64| destination(p0, course, 1.4 * i64_to_f64(s));
        let mut steps = 10_000.0_f64;
        for s in 1..=30 {
            g.on_fix(&raw(at(s), 1000 + s, 4.0), Some(round_i64(steps)));
            steps += 1.93;
        }
        let due = 1200 * 1000;
        g.assignments.push(chain::tests_support::member(9000, 1, "forager", Target::Collect { pts: Vec::new(), need: 3, r: 25.0, theme: "acorns".into() }));
        g.forage.insert(9000, Forage { next_spawn_ms: Some(due), filled: true, ..Forage::default() });
        g.forage_pools.insert(9000, (0..15).map(|i| destination(p0, course, 30.0 * f64::from(i))).collect());
        let mut topped_at = None;
        for s in 31..=330 {
            steps += 1.93;
            g.on_steps(round_i64(steps), (1000 + s) * 1000, None);
            if topped_at.is_none() && !g.forage[&9000].items.is_empty() {
                topped_at = g.position((1000 + s) * 1000).filter(|d| d.source == crate::loc::DisplaySource::Bridged).map(|d| Point::new(d.lat, d.lon));
            }
        }
        let pos = topped_at.expect("topped up while bridged");
        let nearest = g.forage[&9000].items.iter().map(|i| distance_m(i.p, pos)).fold(f64::INFINITY, f64::min);
        assert!(nearest >= ITEM_SPACING_M, "an item {nearest} m from the bridged player");
    }

    #[test]
    fn a_scheduled_top_up_in_a_bridged_gap_keeps_clear_of_where_the_steps_put_the_player() {
        let (mut g, id, p0) = start_near_a_quest();
        let course = crate::geo::bearing_deg(p0, target_of(&g, id));
        let at = |s: i64| destination(p0, course, 1.4 * i64_to_f64(s));
        let mut steps = 10_000.0_f64;
        for s in 1..=30 {
            g.on_fix(&raw(at(s), 1000 + s, 4.0), Some(round_i64(steps)));
            steps += 1.93;
        }
        let due = 1200 * 1000;
        g.assignments.push(chain::tests_support::member(9000, 1, "forager", Target::Collect { pts: Vec::new(), need: 3, r: 25.0, theme: "acorns".into() }));
        g.forage.insert(9000, Forage { next_spawn_ms: Some(due), filled: true, ..Forage::default() });
        g.forage_pools.insert(9000, (0..15).map(|i| destination(p0, course, 30.0 * f64::from(i))).collect());
        for s in 31..=199 {
            steps += 1.93;
            g.on_steps(round_i64(steps), (1000 + s) * 1000, None);
        }
        let pos = g.position(1_199_000).filter(|d| d.source == crate::loc::DisplaySource::Bridged).map(|d| Point::new(d.lat, d.lon));
        let pos = pos.expect("bridged before the wake-up");
        g.tick(due); // the wake-up scheduled for the respawn comes before the next step batch
        assert!(!g.forage[&9000].items.is_empty(), "topped up");
        let nearest = g.forage[&9000].items.iter().map(|i| distance_m(i.p, pos)).fold(f64::INFINITY, f64::min);
        assert!(nearest >= ITEM_SPACING_M, "an item {nearest} m from the bridged player");
    }

    #[test]
    fn near_miss_diagnostics_measure_a_forager_to_its_nearest_open_item() {
        let (mut g, pts) = forager_game();
        g.on_estimate(&fixat(pts[0], 600), None);
        let nm = g.explain_near(&fixat(destination(pts[3], 90.0, 30.0), 1200), 100.0).into_iter().find(|n| n.location_id == 2000);
        assert!(nm.is_some_and(|n| (n.distance_m - 30.0).abs() < 0.5), "its 4th item is 30 m away, its picked first item 300 m");
    }

    #[test]
    fn joining_home_wifi_banks_what_is_carried_once_and_completes_a_forager_at_its_need() {
        let (mut g, pts) = forager_game();
        g.on_estimate(&fixat(pts[0], 600), None);
        g.on_estimate(&fixat(pts[1], 1200), None);
        g.set_counting(false, 0); // home Wi-Fi: no fix inside the home radius is ever accepted
        assert!(done_ids(&g.bank_at_home(1300)).is_empty(), "2 of 3 banked, not done");
        assert_eq!(carried_banked(&g), (0, 2));
        assert!(g.bank_at_home(1400).is_empty());
        assert_eq!(carried_banked(&g), (0, 2), "a second call banks nothing new");
        g.set_counting(true, 0);
        g.on_estimate(&fixat(pts[2], 3000), None);
        assert_eq!(carried_banked(&g), (1, 2), "the tracker carries on from the banked state");
        g.set_counting(false, 0);
        assert_eq!(done_ids(&g.bank_at_home(3600)), vec![2000]);
        assert_eq!(carried_banked(&g), (0, 3));
        assert!(g.bank_at_home(3700).is_empty(), "a finished quest is not completed twice");
    }

    #[test]
    fn a_trap_that_blocks_checks_blocks_banking_at_home_and_keeps_what_is_carried() {
        let (mut g, pts) = forager_game();
        g.on_estimate(&fixat(pts[0], 600), None);
        freeze(&mut g);
        assert!(g.bank_at_home(700).is_empty());
        assert_eq!(carried_banked(&g), (1, 0), "still carried, nothing lost");
        let mut plain = game(&reach_only(&[Mode::Walk], 10, "all_trips"), Backend::Solo, 4);
        assert!(plain.bank_at_home(700).is_empty(), "a game without foragers has nothing to bank");
    }

    #[test]
    fn a_forager_banks_over_several_outings_and_completes_on_the_arrival_that_reaches_the_need() {
        let (mut g, pts) = forager_game();
        g.on_estimate(&fixat(pts[0], 600), None);
        g.on_estimate(&fixat(pts[1], 1200), None);
        assert_eq!(carried_banked(&g), (2, 0));
        assert!(done_ids(&g.on_estimate(&fixat(home(), 1800), None)).is_empty(), "2 of 3 banked");
        assert_eq!(carried_banked(&g), (0, 2));
        g.on_estimate(&fixat(pts[2], 2400), None);
        assert_eq!(done_ids(&g.on_estimate(&fixat(home(), 3000), None)), vec![2000]);
    }

    #[test]
    fn a_shuffle_trap_moves_only_the_unpicked_forager_items_and_keeps_the_counts() {
        let (mut g, pts) = forager_game();
        g.on_estimate(&fixat(pts[0], 600), None);
        g.on_estimate(&fixat(home(), 1200), None);
        g.on_estimate(&fixat(pts[1], 1800), None);
        let before = g.collected[&2000].clone();
        let realms = vec![realm("r0", Mode::Walk)];
        assert_eq!(g.reroll(&[2000], &realms, 9, &Catalog::builtin()).unwrap(), 1);
        assert_eq!(g.collected[&2000], before, "carried, banked and picked are kept");
        assert_eq!(g.assignments[0].kind_id, "forager");
        let Target::Collect { pts: after, need, theme, .. } = g.assignments[0].target.clone() else { panic!("still a forager") };
        assert_eq!((need, theme.as_str()), (3, "acorns"));
        assert_eq!((after[0], after[1]), (pts[0], pts[1]), "picked items stay");
        assert_ne!(after[2..], pts[2..], "unpicked items move");
        g.on_estimate(&fixat(after[2], 2400), None);
        assert_eq!(carried_banked(&g), (2, 1), "a moved item can be picked up");
    }

    /// Regression guard: it passes before the forager branch exists too (2000 is not in the slot, so nothing is re-placed).
    #[test]
    fn a_shuffle_trap_leaves_a_forager_alone_when_its_zone_has_no_room() {
        let (mut g, pts) = forager_game();
        g.on_estimate(&fixat(pts[0], 600), None);
        let (r, _) = realm("r0", Mode::Walk);
        let realms = vec![(r, Atlas::default())];
        assert_eq!(g.reroll(&[2000], &realms, 9, &Catalog::builtin()).unwrap(), 0);
        assert!(matches!(&g.assignments[0].target, Target::Collect { pts: same, .. } if *same == pts));
        assert_eq!(carried_banked(&g), (1, 0));
    }

    #[test]
    fn a_shuffle_trap_refills_a_forager_clear_of_where_the_other_quests_moved_to() {
        let mut g = game(&reach_only(&[Mode::Walk], 10, "all_trips"), Backend::Solo, 4);
        let pts: Vec<Point> = (0..3u32).map(|i| destination(home(), 0.0, 500.0 + 100.0 * f64::from(i))).collect();
        g.assignments.push(chain::tests_support::member(2000, 1, "forager", Target::Collect { pts, need: 3, r: 25.0, theme: "acorns".into() }));
        let id = g
            .assignments
            .iter()
            .find(|a| a.location_id != 2000 && !g.done.contains(&a.location_id) && !is_chain_target(&a.target) && anchor(&a.target).is_some())
            .expect("an unfinished quest with a point")
            .location_id;
        // Where it was is far from where it goes, so only the order of the refill decides which of the two the items keep clear of.
        g.assignments.iter_mut().find(|a| a.location_id == id).unwrap().target = Target::Point { p: destination(home(), 180.0, 5000.0), r: 40.0 };
        let realms = vec![realm("r0", Mode::Walk)];
        let moved_to = |g: &Game| anchor(&g.assignments.iter().find(|a| a.location_id == id).unwrap().target).unwrap();
        let mut probe: Game = serde_json::from_value(serde_json::to_value(&g).unwrap()).unwrap();
        probe.reroll(&[2000, id], &realms, 9, &Catalog::builtin()).unwrap();
        let there = moved_to(&probe);
        // Only spots by where the other quest lands, and a few far from everything.
        let pool: Vec<Point> =
            (0..6).map(|i| destination(there, 60.0 * f64::from(i), 10.0)).chain((0..3).map(|i| destination(home(), 120.0 * f64::from(i), 3000.0))).collect();
        g.forage_pools.insert(2000, pool);
        g.reroll(&[2000, id], &realms, 9, &Catalog::builtin()).unwrap();
        assert_eq!(moved_to(&g), there);
        assert!(g.forage[&2000].items.iter().all(|i| distance_m(i.p, there) >= ITEM_SPACING_M), "an item on the quest that just moved there");
    }

    #[test]
    fn a_shuffle_trap_drops_stale_forager_progress_of_a_quest_it_places_fresh() {
        let mut g = game(&reach_only(&[Mode::Walk], 10, "all_trips"), Backend::Solo, 4);
        let id = g.assignments.iter().map(|a| a.location_id).find(|i| !g.done.contains(i)).expect("an unfinished quest");
        g.collected.insert(id, Collected { carried: 2, banked: 1, ..Collected::default() });
        let realms = vec![realm("r0", Mode::Walk)];
        g.reroll(&[id], &realms, 9, &Catalog::builtin()).unwrap();
        assert!(!g.collected.contains_key(&id), "a different quest must not inherit the old counts");
    }

    #[test]
    fn forager_progress_survives_a_restart_and_an_old_save_without_it_loads() {
        let dir = std::env::temp_dir().join(format!("apgo-forager-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let (mut g, pts) = forager_game();
        g.on_estimate(&fixat(pts[0], 600), None);
        g.on_estimate(&fixat(home(), 1200), None);
        g.on_estimate(&fixat(pts[1], 1800), None);
        g.save(&dir).unwrap();
        let mut back = Game::load(&dir, "g1").unwrap();
        assert_eq!(back.collected, g.collected);
        let v = back.quest_views(1800).remove(0);
        assert!((v.progress - 0.5).abs() < 1e-6 && v.state == QuestState::InProgress, "(1 + 0.5) / 3 before any new fix: {}", v.progress);
        assert_eq!(v.collected, g.collected.get(&2000).cloned());
        back.on_estimate(&fixat(home(), 2400), None);
        assert_eq!(carried_banked(&back), (0, 2), "what was carried before the restart is banked");
        let _ = std::fs::remove_dir_all(&dir);

        let mut old = serde_json::to_value(&g).unwrap();
        old.as_object_mut().unwrap().remove("collected");
        let loaded: Game = serde_json::from_value(old).unwrap();
        assert!(loaded.collected.is_empty());
    }

    #[test]
    fn nothing_is_picked_or_banked_while_counting_is_off_or_a_freeze_trap_blocks_checks() {
        let (mut g, pts) = forager_game();
        g.set_counting(false, 0);
        g.on_estimate(&fixat(pts[0], 600), None);
        assert_eq!(carried_banked(&g), (0, 0), "counting off: no pickup");
        g.set_counting(true, 0);
        freeze(&mut g);
        g.on_estimate(&fixat(pts[5], 1200), None);
        assert_eq!(carried_banked(&g), (0, 0), "frozen: no pickup");
        // Clear the trap, pick one, freeze again: a home fix must not bank either.
        g.traps.active.clear();
        g.on_estimate(&fixat(pts[4], 1350), None);
        assert_eq!(carried_banked(&g), (1, 0));
        freeze(&mut g);
        g.on_estimate(&fixat(home(), 1500), None);
        assert_eq!(carried_banked(&g), (1, 0), "frozen: no banking");
    }

    #[test]
    fn carried_items_wait_out_home_wifi_and_bank_on_the_next_accepted_home_fix() {
        let (mut g, pts) = forager_game();
        g.on_estimate(&fixat(pts[0], 600), None);
        g.set_counting(false, 0); // home Wi-Fi before the arrival fix
        g.on_estimate(&fixat(home(), 1200), None);
        assert_eq!(carried_banked(&g), (1, 0), "still carried");
        g.set_counting(true, 0); // leaving home on the next walk
        g.on_estimate(&fixat(destination(home(), 0.0, 60.0), 5000), None);
        assert_eq!(carried_banked(&g), (0, 1));
    }

    fn wanderlust(g: &Game, now_ms: i64) -> f64 {
        g.chain_views(now_ms).into_iter().find(|c| c.id == "1:wanderlust").unwrap().counter
    }

    #[test]
    fn leaving_home_opens_one_outing_that_a_car_ride_keeps_open() {
        let mut g = chain_game("step_up", vec![Target::Steps { n: 500 }]);
        g.set_counting(false, 0);
        g.set_counting(true, 1_000);
        let o = g.outing.clone().expect("left home");
        assert_eq!((o.start_ms, o.steps, o.items_at_start), (1_000, 0, g.items.len()));
        g.set_counting(false, 2_000); // in the car
        g.set_counting(true, 3_000);
        assert_eq!(g.outing.as_ref().map(|o| o.start_ms), Some(1_000), "the car ride does not start a new walk");
    }

    #[test]
    fn counting_on_at_home_opens_no_outing_until_a_fix_away() {
        let mut g = chain_game("step_up", vec![Target::Steps { n: 500 }]);
        g.set_counting(false, 0);
        g.counters.last_at_home = Some(true);
        g.set_counting(true, 1_000);
        assert!(g.outing.is_none());
        g.on_estimate(&fixat(destination(g.home, 90.0, 500.0), 60), None);
        assert_eq!(g.outing.as_ref().map(|o| o.start_ms), Some(60_000));
    }

    #[test]
    fn an_outing_counts_raw_steps_even_with_a_heavy_backpack() {
        let (mut g, pts) = forager_game();
        g.on_estimate(&fixat(pts[0], 600), None);
        g.on_estimate(&fixat(pts[1], 1200), None);
        g.bag_size = 0; // two items over the bag: credited steps slow down
        g.set_counting(false, 1_300_000);
        g.set_counting(true, 1_400_000);
        g.on_steps(0, 1_500_000, None);
        g.on_steps(800, 1_600_000, None);
        assert_eq!(g.outing.as_ref().map(|o| o.steps), Some(800));
    }

    #[test]
    fn an_outing_survives_a_restart_and_old_saves_have_none() {
        let mut g = chain_game("step_up", vec![Target::Steps { n: 500 }]);
        g.set_counting(false, 0);
        g.set_counting(true, 1_000);
        let back: Game = serde_json::from_str(&serde_json::to_string(&g).unwrap()).unwrap();
        assert_eq!(back.outing, g.outing, "unlike time away, the walk is kept across a restart");
        let mut old = serde_json::to_value(&g).unwrap();
        old.as_object_mut().unwrap().remove("outing");
        old.as_object_mut().unwrap().remove("recap");
        let old: Game = serde_json::from_value(old).unwrap();
        assert!(old.outing.is_none() && old.recap.is_none());
    }

    #[test]
    fn time_away_runs_from_leaving_home_with_no_fixes_needed() {
        let mut g = away_game(&[3.0, 2.0]); // marks at 2 and 5 minutes
        g.set_counting(false, 0); // at home (Wi-Fi connected)
        g.set_counting(true, 60_000); // Wi-Fi dropped: away from minute 1
        assert!((wanderlust(&g, 150_000) - 1.5).abs() < 0.01, "worked out when asked; nothing ran meanwhile");
        assert!(g.away_running(), "a screen showing it may redraw now and then");
        assert_eq!(g.next_due_ms(150_000), Some(180_000), "the 2-minute mark falls due at minute 3");
        assert_eq!(done_ids(&g.tick(180_000)), vec![1001]);
        assert_eq!(g.next_due_ms(180_000), Some(360_000));
        g.set_counting(false, 240_000); // home again at minute 4: three minutes banked
        assert!((g.counters.progress["1:wanderlust"] - 3.0).abs() < 0.01);
        assert_eq!(g.next_due_ms(300_000), None, "at home nothing falls due");
        assert!(!g.away_running(), "at home the live value is not moving");
        assert!((wanderlust(&g, 999_000) - 3.0).abs() < 0.01, "time at home does not count");
    }

    #[test]
    fn a_dwell_quest_finishes_on_its_scheduled_tick_without_new_fixes() {
        let mut g = game(&reach_only(&[Mode::Walk], 10, "all_trips"), Backend::Solo, 4);
        let spot = destination(g.home, 90.0, 1200.0);
        g.assignments.push(chain::tests_support::member(9000, 1, "bench_warmer", Target::Dwell { p: spot, r: 40.0, minutes: 3.0 }));
        assert!(g.on_estimate(&fixat(spot, 1000), None).iter().all(|e| !matches!(e, Event::QuestDone { location_id: 9000, .. })));
        assert_eq!(g.next_due_ms(1_000_000), Some(1_180_000), "three minutes after arriving");
        assert!(g.tick(1_180_000).contains(&Event::QuestDone { location_id: 9000, name: g.assignments.last().unwrap().quest_name.clone() }));
    }

    #[test]
    fn a_dwell_is_not_finished_by_a_tick_after_unusable_fixes() {
        let mut g = game(&reach_only(&[Mode::Walk], 10, "all_trips"), Backend::Solo, 4);
        let spot = destination(g.home, 90.0, 1200.0);
        g.assignments.push(chain::tests_support::member(9000, 1, "bench_warmer", Target::Dwell { p: spot, r: 40.0, minutes: 3.0 }));
        g.on_estimate(&fixat(spot, 1000), None);
        // Walking off under trees: only blurry fixes, which the trackers never see.
        g.on_estimate(&Estimate { accepted: false, verdict: LocVerdict::Blurry, uncertainty_m: 80.0, ..fixat(destination(spot, 0.0, 300.0), 1060) }, None);
        assert_eq!(g.next_due_ms(1_100_000), None, "no wake-up for a dwell that may have been left");
        assert!(g.tick(1_180_000).iter().all(|e| !matches!(e, Event::QuestDone { location_id: 9000, .. })));
        g.on_estimate(&fixat(spot, 1100), None); // a good fix: still there after all
        assert_eq!(g.next_due_ms(1_100_000), Some(1_180_000));
    }

    #[test]
    fn a_long_stretch_without_fixes_still_counts_as_time_away() {
        let mut g = away_game(&[10.0]);
        g.set_counting(true, 0);
        away_for(&mut g, 1500.0, 0, 1);
        away_for(&mut g, 1500.0, 540, 1); // nine quiet minutes (indoors, standing still): still away
        assert!((g.counters.progress["1:wanderlust"] - 9.0).abs() < 0.01);
    }

    #[test]
    fn without_home_wifi_a_fix_near_home_ends_time_away() {
        let mut g = away_game(&[10.0]);
        g.set_counting(true, 0);
        away_for(&mut g, 400.0, 0, 3); // 400 m is away now: two minutes
        away_for(&mut g, 50.0, 180, 1); // home: the stretch since the last fix is not credited
        assert!((g.counters.progress["1:wanderlust"] - 2.0).abs() < 0.01);
        assert_eq!(g.next_due_ms(200_000), None);
        away_for(&mut g, 400.0, 300, 1); // out again
        assert!((wanderlust(&g, 360_000) - 3.0).abs() < 0.01);
    }

    #[test]
    fn counting_coming_back_on_at_home_does_not_start_time_away() {
        // No home Wi-Fi: only fixes say you are home. A car ride ends in the driveway: counting goes off and on again with no new fix.
        let mut g = away_game(&[10.0]);
        away_for(&mut g, 30.0, 0, 1); // at home
        g.set_counting(false, 60_000); // car Bluetooth connects
        g.set_counting(true, 120_000); // and disconnects at home
        assert!(g.counters.away_mark.is_none(), "the last fix said home, so time away does not start");
        assert_eq!(g.next_due_ms(200_000), None);
        away_for(&mut g, 400.0, 300, 1); // walking out starts it
        assert!(g.counters.away_mark.is_some());
    }

    #[test]
    fn a_restart_does_not_count_the_time_the_app_was_dead() {
        let dir = std::env::temp_dir().join(format!("apgo-away-restart-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut g = away_game(&[600.0]);
        g.set_counting(true, 0);
        g.save(&dir).unwrap();
        let mut back = Game::load(&dir, "g1").unwrap();
        assert!(back.counters.away_mark.is_none(), "after a restart where you are is unknown");
        assert!(wanderlust(&back, 36_000_000).abs() < 0.01, "ten hours with the app dead are not time away");
        back.set_counting(false, 36_000_000); // back home when the app came back
        assert!(back.counters.progress.get("1:wanderlust").is_none_or(|m| *m == 0.0));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn one_unbroken_stretch_counts_at_most_two_hours_until_the_next_event() {
        let mut g = away_game(&[600.0]); // one mark, ten hours out
        g.set_counting(true, 0);
        assert_eq!(g.next_due_ms(0), Some(AWAY_MAX_STRETCH_MS), "a wake-up at the cap keeps a real outing counting");
        assert!((wanderlust(&g, 5 * 3_600_000) - 120.0).abs() < 0.01, "a missed event never credits more than the cap");
        g.tick(AWAY_MAX_STRETCH_MS);
        assert!((wanderlust(&g, 3 * 3_600_000) - 180.0).abs() < 0.01, "the next stretch runs on from the tick");
    }

    #[test]
    fn the_game_being_played_survives_a_restart_until_paused() {
        let dir = std::env::temp_dir().join(format!("apgo-playing-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(Game::playing(&dir), None, "nothing played yet");
        let g = game(&reach_only(&[Mode::Walk], 10, "all_trips"), Backend::Solo, 4);
        g.save(&dir).unwrap();
        Game::mark_playing(&dir, "g1").unwrap();
        assert_eq!(Game::playing(&dir), Some("g1".to_string()), "read back as a fresh start would");
        Game::clear_playing(&dir);
        assert_eq!(Game::playing(&dir), None, "paused: nothing to resume");
        Game::clear_playing(&dir); // clearing twice is fine
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_deleted_game_is_never_resumed() {
        let dir = std::env::temp_dir().join(format!("apgo-playing-gone-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let g = game(&reach_only(&[Mode::Walk], 10, "all_trips"), Backend::Solo, 4);
        g.save(&dir).unwrap();
        Game::mark_playing(&dir, "g1").unwrap();
        Game::archive(&dir, "g1").unwrap();
        assert_eq!(Game::playing(&dir), None);
        Game::mark_playing(&dir, "never-saved").unwrap();
        assert_eq!(Game::playing(&dir), None, "an id with no save is ignored");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Save `g` and load it back, as an app restart does.
    fn restarted(g: &Game, name: &str) -> Game {
        let dir = std::env::temp_dir().join(format!("apgo-restart-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        g.save(&dir).unwrap();
        let back = Game::load(&dir, &g.id).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        back
    }

    #[test]
    fn a_courier_pickup_survives_a_restart() {
        let (a, b) = (destination(home(), 0.0, 400.0), destination(home(), 90.0, 800.0));
        let mut g = chain_game("courier", vec![Target::Courier { a, b, r: 40.0, time_limit_min: 30.0 }]);
        g.on_estimate(&fixat(a, 10), None); // picked up
        let mut back = restarted(&g, "courier");
        let ev = back.on_estimate(&fixat(b, 300), None);
        assert_eq!(done_ids(&ev), vec![1000], "the pickup is kept: {ev:?}");
    }

    #[test]
    fn a_dwells_best_stretch_survives_a_restart_but_its_running_timer_does_not() {
        let mut g = chain_game("dwell", vec![Target::Dwell { p: home(), r: 50.0, minutes: 10.0 }]);
        g.on_estimate(&fixat(home(), 0), None);
        g.on_estimate(&fixat(home(), 300), None); // 5 of 10 minutes
        let mut back = restarted(&g, "dwell");
        assert_eq!(back.next_due_ms(400_000), None, "where the player was while the app was dead is unknown");
        let ev = back.on_estimate(&fixat(home(), 3600), None);
        assert!(ev.is_empty(), "an hour with the app dead is not dwell time: {ev:?}");
        assert!(matches!(back.trackers.get(&1000).map(Tracker::status), Some(Status::Active(p)) if p >= 0.5), "the best stretch is kept");
    }

    #[test]
    fn a_dwell_started_under_a_slow_trap_is_back_to_normal_after_a_restart_once_the_trap_is_over() {
        let mut g = chain_game("dwell", vec![Target::Dwell { p: home(), r: 50.0, minutes: 10.0 }]);
        g.traps.active.push(crate::traps::Trap::Slow { until_ms: 400_000 });
        g.on_estimate(&fixat(home(), 0), None);
        g.on_estimate(&fixat(home(), 300), None); // 5 of 20 minutes
        g.traps.active.clear(); // the trap ran out
        let mut back = restarted(&g, "slow-dwell");
        back.on_estimate(&fixat(home(), 1000), None);
        let ev = back.on_estimate(&fixat(home(), 1600), None);
        assert_eq!(done_ids(&ev), vec![1000], "ten minutes again, not twenty: {ev:?}");
    }

    #[test]
    fn an_unreadable_tracker_is_dropped_and_the_game_still_loads() {
        let (a, b) = (destination(home(), 0.0, 400.0), destination(home(), 90.0, 800.0));
        let mut g = chain_game("courier", vec![Target::Courier { a, b, r: 40.0, time_limit_min: 30.0 }]);
        g.on_estimate(&fixat(a, 10), None);
        let mut json: serde_json::Value = serde_json::to_value(&g).unwrap();
        json["trackers"]["777"] = serde_json::json!({ "state": { "Renamed": {} }, "done": false, "progress": 0.0 });
        let dir = std::env::temp_dir().join(format!("apgo-bad-tracker-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(Game::path_for(&dir, &g.id).parent().unwrap()).unwrap();
        std::fs::write(Game::path_for(&dir, &g.id), json.to_string()).unwrap();
        let back = Game::load(&dir, &g.id).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        assert!(!back.trackers.contains_key(&777), "the unreadable one is dropped");
        assert!(back.trackers.contains_key(&1000), "the good one is kept");
    }

    #[test]
    fn a_save_without_trackers_still_loads() {
        let g = chain_game("courier", vec![Target::Courier { a: home(), b: home(), r: 40.0, time_limit_min: 30.0 }]);
        let mut json: serde_json::Value = serde_json::to_value(&g).unwrap();
        json.as_object_mut().unwrap().remove("trackers");
        let old: Game = serde_json::from_value(json).unwrap();
        assert!(old.trackers.is_empty());
    }

    #[test]
    fn near_miss_reasons_read_in_the_players_units() {
        let (mut g, id, _) = start_near_a_quest();
        g.set_units(UnitSystem::Imperial);
        let target = g.assignments.iter().find(|a| a.location_id == id).and_then(|a| anchor(&a.target)).unwrap();
        let nm = g.explain_near(&fixat(destination(target, 0.0, 70.0), 1000), 100.0).into_iter().find(|n| n.location_id == id).unwrap();
        assert_eq!(nm.reason, "230 ft away, needs 130 ft");
    }

    #[test]
    fn a_near_miss_never_reads_as_if_it_met_the_limit() {
        let (g, id, _) = start_near_a_quest();
        let target = g.assignments.iter().find(|a| a.location_id == id).and_then(|a| anchor(&a.target)).unwrap();
        let reason = |m: f64| g.explain_near(&fixat(destination(target, 0.0, m), 1000), 100.0).into_iter().find(|n| n.location_id == id).unwrap().reason;
        assert_eq!(reason(42.0), "45 m away, needs 40 m", "42 m must not read as 40 m");
        assert!(reason(38.0).starts_with("in range (35 m, needs 40 m)"), "{}", reason(38.0));
    }

    #[test]
    fn a_new_game_starts_with_empty_counters_and_no_time_away() {
        let g = game(&reach_only(&[Mode::Walk, Mode::Run], 20, "all_trips"), Backend::Solo, 4);
        assert!(g.counters.progress.is_empty() && g.counters.steps_last.is_none() && g.counters.away_mark.is_none());
    }

    fn left_home(g: &mut Game, t_ms: i64) {
        g.set_counting(false, 0);
        g.set_counting(true, t_ms);
    }

    #[test]
    fn arriving_home_after_a_walk_gives_its_recap_once() {
        let mut g = chain_game("step_up", vec![Target::Steps { n: 500 }]);
        left_home(&mut g, 0);
        g.on_steps(0, 1, None);
        g.on_steps(600, 2, None); // the 500-step member is done; solo reward "Hydrate!"
        let (_, r) = g.arrive_home(10 * 60_000);
        let r = r.expect("worth showing");
        assert_eq!((r.start_ms, r.end_ms, r.steps), (0, 600_000, 600));
        assert_eq!(r.quests_done.len(), 1);
        assert_eq!(r.items, vec!["Hydrate!".to_string()]);
        assert_eq!(r.checks_sent, 0, "a solo game sends no checks");
        assert!(g.outing.is_none());
        assert!(g.arrive_home(11 * 60_000).1.is_none(), "a second arrival has no outing to close");
    }

    #[test]
    fn a_short_or_idle_outing_closes_without_a_recap() {
        let mut g = chain_game("step_up", vec![Target::Steps { n: 5000 }]);
        left_home(&mut g, 0);
        g.on_steps(0, 1, None);
        g.on_steps(600, 2, None);
        assert!(g.arrive_home(4 * 60_000).1.is_none(), "under 5 min");
        assert!(g.outing.is_none(), "closed all the same");
        left_home(&mut g, 10 * 60_000);
        assert!(g.outing.is_some(), "a new outing opened");
        assert!(g.arrive_home(30 * 60_000).1.is_none(), "long, nothing happened");
    }

    #[test]
    fn a_zone_unlocked_on_the_walk_is_in_the_recap() {
        let mut g = cartographer_game();
        lock_zone_1(&mut g);
        left_home(&mut g, 0);
        g.receive_item("Progressive Zone Key", 1_000, None);
        let r = g.arrive_home(10 * 60_000).1.expect("a zone opened");
        assert_eq!(r.zones_unlocked, vec![1]);
    }

    #[test]
    fn a_shrunk_item_list_gives_no_items_instead_of_panicking() {
        let mut g = chain_game("step_up", vec![Target::Steps { n: 500 }]);
        g.items = vec!["a".into(), "b".into()];
        left_home(&mut g, 0);
        g.items.clear(); // the server's list was replaced by a shorter one
        g.outing.as_mut().unwrap().steps = 900;
        assert!(g.arrive_home(10 * 60_000).1.is_some_and(|r| r.items.is_empty()));
    }

    #[test]
    fn arriving_home_banks_and_the_recap_counts_what_was_banked() {
        let (mut g, pts) = forager_game();
        left_home(&mut g, 0);
        g.on_estimate(&fixat(pts[0], 600), None);
        g.on_estimate(&fixat(pts[1], 1200), None);
        g.set_counting(false, 1_250_000); // home Wi-Fi
        let r = g.arrive_home(1_300_000).1.expect("two items brought home");
        assert_eq!(r.banked, 2);
        assert_eq!(carried_banked(&g), (0, 2));
    }

    #[test]
    fn items_banked_by_a_fix_inside_the_home_radius_count_as_brought_home() {
        let (mut g, pts) = forager_game();
        left_home(&mut g, 0);
        for (i, p) in pts[..3].iter().enumerate() {
            g.on_estimate(&fixat(*p, 600 * (i as i64 + 1)), None);
        }
        assert_eq!(done_ids(&g.on_estimate(&fixat(home(), 1_250_000), None)), vec![2000], "the home fix banks and completes it");
        g.set_counting(false, 1_260_000); // presence reaches "at home" after the fix
        let r = g.arrive_home(1_300_000).1.expect("three items brought home");
        assert_eq!(r.banked, 3);
        assert_eq!(r.quests_done, vec![g.assignments[0].quest_name.clone()]);
    }

    #[test]
    fn an_outing_saved_before_banked_at_start_still_loads() {
        let old = serde_json::json!({ "start_ms": 1, "steps": 2, "cells_at_start": 0, "done_at_start": [], "items_at_start": 0, "zones_at_start": [] });
        assert_eq!(serde_json::from_value::<Outing>(old).unwrap().banked_at_start, 0);
    }

    #[test]
    fn a_trap_blocking_the_bank_still_closes_the_walk() {
        let (mut g, pts) = forager_game();
        left_home(&mut g, 0);
        g.on_estimate(&fixat(pts[0], 600), None);
        g.outing.as_mut().unwrap().steps = 900;
        freeze(&mut g); // a Freeze Trap blocks checks, so banking
        let r = g.arrive_home(1_300_000).1.expect("steps alone are worth it");
        assert_eq!(r.banked, 0, "nothing banked under the trap");
        assert!(g.outing.is_none());
    }
}
