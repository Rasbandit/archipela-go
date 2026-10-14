//! `UniFFI` facade over the game engine: realms, scanning, game setup, play. Blocking calls; call from a background thread.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::PathBuf;
use std::sync::atomic::AtomicU64;
use std::sync::{Arc, Mutex, MutexGuard};

use apgo_core::assign::SurfacePref;
use apgo_core::assign::{Assignment, Target};
use apgo_core::card;
use apgo_core::catalog::{Catalog, Kind, Mode};
use apgo_core::chain::ChainUnit;
use apgo_core::game::{ApServer, Backend, Event, Game, NearMiss, NewGame, QuestState, Streets};
use apgo_core::geo::{distance_m, simplify_spaced, Point};
use apgo_core::home_watch::{HomeState, HomeWatch};
use apgo_core::journal::{kind, Journal, JournalEvent, TrackPoint, DEFAULT_MAX_GAP_MS};
use apgo_core::loc::calib::StepCal;
use apgo_core::loc::{DisplayPosition, DisplaySource, Estimate, HeadingSource, Provider, RawFix, Verdict};
use apgo_core::marks::Mark;
use apgo_core::num::count_u32;
use apgo_core::realm::{Proximity, Realm, RealmStore, Shape};
use apgo_core::recap::Recap;
use apgo_core::save_policy::SavePolicy;
use apgo_core::scan::{scan_with_stop, Atlas, Fetch, Pacing, SCAN_BUDGET};
use apgo_core::settings::{resolve_units, Settings};
use apgo_core::slot::SlotData;
use apgo_core::solo::{generate, SoloOptions};
use apgo_core::units::{distance, distance_rounded, Round, UnitSystem};
use apgo_core::verify::Collected;
use apgo_core::yaml::build_yaml;

use crate::{CoreError, GeoPoint};

#[allow(clippy::needless_pass_by_value)] // used as a `map_err` callback, which hands over the error by value
fn err<E: ToString>(e: E) -> CoreError {
    CoreError::Failed { detail: e.to_string() }
}

fn line(pts: Vec<Point>) -> TrackSegmentOut {
    TrackSegmentOut { points: pts.into_iter().map(gp).collect() }
}

fn gp(p: Point) -> GeoPoint {
    GeoPoint { lat: p.lat, lon: p.lon }
}

fn pt(p: &GeoPoint) -> Point {
    Point::new(p.lat, p.lon)
}

/// What [`Engine::refresh_streets`] built and swapped into the open game.
#[derive(Debug, uniffi::Record)]
pub struct StreetsBuiltOut {
    /// Time to read the scans and build the street index and graph, ms.
    pub build_ms: u32,
    /// Segments in the street graph (0 without streets).
    pub segments: u32,
    /// Whether part of the graph comes from a scan made before way geometry was recorded.
    pub degraded: bool,
}

/// A circle: a middle and a radius.
#[derive(Debug, uniffi::Record)]
pub struct CircleOut {
    /// Middle of the circle.
    pub center: GeoPoint,
    /// Radius in metres.
    pub radius_m: f64,
}

/// A realm as the UI shows it.
#[derive(Debug, uniffi::Record)]
pub struct RealmOut {
    /// Realm id.
    pub id: String,
    /// Realm name.
    pub name: String,
    /// The icon picked for the realm, if any.
    pub icon: Option<String>,
    /// The circle if the realm has one (active or kept in reserve).
    pub circle: Option<CircleOut>,
    /// The polygon corners (empty if none), active or kept in reserve.
    pub polygon: Vec<GeoPoint>,
    /// Which of the two is the realm's real outline.
    pub polygon_active: bool,
    /// When the realm was last scanned, in Unix milliseconds.
    pub scanned_at_ms: Option<u64>,
    /// Number of scanned places that can serve quests.
    pub places: u32,
    /// Set when a scan stopped early (slow public map servers); Rescan continues from the cache.
    pub warning: Option<String>,
}

/// A find reduced to what a small map preview needs.
#[derive(Debug, uniffi::Record)]
pub struct DotOut {
    /// Where the find is.
    pub at: GeoPoint,
    /// Catalog id of the first quest kind.
    pub kind_id: String,
    /// Family of the first quest kind.
    pub family: String,
}

/// A quest kind a find can serve, with what it means and how it is completed.
#[derive(Debug, uniffi::Record)]
pub struct KindOut {
    /// Catalog id of the quest kind.
    pub id: String,
    /// Display name of the quest kind.
    pub name: String,
    /// Quest family.
    pub family: String,
    /// Short description.
    pub blurb: String,
    /// What the player has to do ("Get within 40 m.").
    pub how: String,
}

/// One find: a scanned spot a realm can use for quests, with the player's mark on it.
#[derive(Debug, uniffi::Record)]
pub struct FindOut {
    /// Stable id of the find (the OpenStreetMap feature).
    pub id: String,
    /// The find's own name, or the name of its first quest kind when it has none ("Bench Warmer").
    pub name: String,
    /// Whether the find has a name of its own.
    pub named: bool,
    /// The quest kinds this find can serve.
    pub kinds: Vec<KindOut>,
    /// The `key=value` map tags that made it match, for the curious ("leisure=pitch").
    pub tags: Vec<String>,
    /// The first quest kind's id and family, for choosing an icon.
    pub kind_id: String,
    /// Quest family.
    pub family: String,
    /// Where the find is.
    pub at: GeoPoint,
    /// Distance from home in metres.
    pub distance_m: f64,
    /// "none" | "favorite" | "banned"
    pub mark: String,
}

/// Measurements of a shape that need no scan: they can follow a drag live.
#[derive(Debug, uniffi::Record)]
pub struct ShapeStatsOut {
    /// Area in square metres.
    pub area_m2: f64,
    /// Length of the outline in metres.
    pub perimeter_m: f64,
    /// How far the farthest part of the shape is from home in a straight line.
    pub farthest_m: f64,
}

/// What the scan found in a realm.
#[derive(Debug, uniffi::Record)]
pub struct RealmStatsOut {
    /// Total length of walkable streets and paths.
    pub walkable_m: f64,
    /// Share of that which is rough going (unpaved, unknown surface, stairs), 0 to 1.
    pub rough_share: f64,
    /// Total length of the trails (named paths and the like) that can serve quests.
    pub trail_m: f64,
    /// Number of parks.
    pub parks: u32,
    /// How many differently named streets there are.
    pub streets: u32,
    /// Number of scanned places that can serve quests.
    pub finds: u32,
    /// How many quest kinds the realm can offer.
    pub quest_types: u32,
}

/// What a scan would cost.
#[derive(Debug, uniffi::Record)]
pub struct ScanPlanOut {
    /// Number of map tiles.
    pub tiles: u32,
    /// Number of map requests in all.
    pub requests: u32,
    /// Requests that are not in the cache and would go to the network.
    pub missing: u32,
}

/// Told how a realm scan is going, as each map request finishes (called on the scanning thread): the app shows progress from these
/// calls instead of polling.
#[uniffi::export(with_foreign)]
pub trait ScanListener: Send + Sync {
    /// Requests finished so far out of all of them.
    fn progress(&self, done: u32, total: u32);
    /// Asked before each map request: false stops the scan (e.g. the realm was hidden to be deleted). Nothing is saved then.
    fn keep_going(&self) -> bool;
}

/// A quest kind a realm can offer, with how many places serve it.
#[derive(Debug, uniffi::Record)]
pub struct OfferOut {
    /// Catalog id of the quest kind.
    pub kind_id: String,
    /// Display name of the quest kind.
    pub name: String,
    /// Quest family.
    pub family: String,
    /// Short description.
    pub blurb: String,
    /// How many places in the realm can serve the kind.
    pub count: u32,
}

/// One goal's progress, for the Play screen.
#[derive(Debug, uniffi::Record)]
pub struct GoalLineOut {
    /// Goal id.
    pub id: String,
    /// Text describing the goal.
    pub label: String,
    /// Progress from 0 to 1.
    pub progress: f32,
    /// Whether the goal is met.
    pub achieved: bool,
}

/// The options of a solo game, as chosen in New Game.
#[derive(Debug, uniffi::Record)]
#[allow(clippy::struct_excessive_bools)] // mirrors the YAML options the UI sends, each an independent switch
pub struct SoloOptionsIn {
    /// Ids of the win conditions (at least one).
    pub goals: Vec<String>,
    /// How they combine: "any", "all" or "`at_least`".
    pub goal_requirement: String,
    /// For "`at_least"`: how many of the goals must be finished.
    pub goal_need: u32,
    /// How many quest locations the game has.
    pub number_of_trips: u32,
    /// How many zones (1 to 6).
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
    /// Quest types for each zone, in zone order; an empty list uses `quest_types`.
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

fn to_core(o: SoloOptionsIn) -> Result<SoloOptions, CoreError> {
    let goal_mode = match o.goal_requirement.as_str() {
        "any" => apgo_core::slot::GoalMode::Any,
        "all" => apgo_core::slot::GoalMode::All,
        "at_least" => apgo_core::slot::GoalMode::AtLeast,
        other => return Err(err(format!("unknown goal requirement {other}"))),
    };
    Ok(SoloOptions {
        goal: String::new(),
        goals: o.goals,
        goal_mode,
        goal_need: o.goal_need,
        zone_quest_types: o.zone_quest_types,
        number_of_trips: o.number_of_trips,
        zone_count: o.zone_count,
        run_quests: o.run_quests,
        run_share: o.run_share,
        easy_share: o.easy_share,
        medium_share: o.medium_share,
        hard_share: o.hard_share,
        minutes_per_tier: o.minutes_per_tier,
        min_distance_m: o.min_distance_m,
        quest_types: o.quest_types,
        enabled_traps: o.enabled_traps,
        trap_rate: o.trap_rate,
        enable_effort_reductions: o.enable_effort_reductions,
        enable_scouting: o.enable_scouting,
        enable_collection: o.enable_collection,
        reduction_percent: o.reduction_percent,
        fog_of_war: o.fog_of_war,
        return_home: o.return_home,
    })
}

/// A saved game in the list.
#[derive(Debug, uniffi::Record)]
pub struct GameInfo {
    /// Unique id of the game.
    pub id: String,
    /// Display name of the game.
    pub name: String,
}

/// One badge (a tool a quest or chain needs), worded and iconed by the core.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BadgeOut {
    /// The tool item ("Running Shoes").
    pub item: String,
    /// Which icon to draw: "run", or "tool" for any other.
    pub icon: String,
    /// Whether the player does not hold it yet.
    pub missing: bool,
}

fn badge_out(b: apgo_core::game::BadgeView) -> BadgeOut {
    BadgeOut { item: b.item, icon: b.icon, missing: b.missing }
}

/// One milestone of a progressive chain.
#[derive(Debug, uniffi::Record)]
pub struct MarkOut {
    /// Counter value at which the milestone is reached.
    pub at: f64,
    /// Archipelago location id of the check it unlocks.
    pub location_id: i64,
    /// Whether the counter has reached it.
    pub reached: bool,
    /// Solo only: what the milestone gave, once reached.
    pub reward: Option<String>,
}

/// A progressive quest chain: one counter with a check at each milestone.
#[derive(Debug, uniffi::Record)]
pub struct ChainOut {
    /// Chain id.
    pub id: String,
    /// Zone number the chain is in.
    pub zone: u32,
    /// Catalog id of the quest kind.
    pub kind_id: String,
    /// Display name of the chain.
    pub name: String,
    /// Quest family.
    pub family: String,
    /// steps | minutes | cells
    pub unit: String,
    /// Current counter value.
    pub counter: f64,
    /// Counter value at the last milestone.
    pub total: f64,
    /// Short rule text for the chain.
    pub rule: String,
    /// The milestones in order.
    pub marks: Vec<MarkOut>,
    /// The chain's badges (the chain counts nothing while any is missing).
    pub badges: Vec<BadgeOut>,
    /// What it still needs ("Needs Running Shoes"); `None` when nothing is missing.
    pub badge_note: Option<String>,
}

/// A quest as the UI shows it.
#[derive(Debug, uniffi::Record)]
pub struct QuestOut {
    /// Archipelago location id of the check.
    pub location_id: i64,
    /// Zone number the quest is in.
    pub zone: u32,
    /// Display name of the quest.
    pub name: String,
    /// Name of the place the quest uses.
    pub place: String,
    /// Quest family.
    pub family: String,
    /// Catalog id of the quest kind.
    pub kind_id: String,
    /// Difficulty band: easy, medium or hard.
    pub difficulty: String,
    /// Effort tier, starting at 1.
    pub tier: u8,
    /// Expected effort in minutes.
    pub effort_min: f64,
    /// How the player travels there: walk or run.
    pub mode: String,
    /// The quest's badges: tools that must all be held before it can be played (it is locked while any is missing).
    pub badges: Vec<BadgeOut>,
    /// The badge its map pin shows; `None` for an open quest.
    pub pin_badge: Option<BadgeOut>,
    /// What it still needs, for the card ("Needs Running Shoes"); `None` when nothing is missing.
    pub badge_note: Option<String>,
    /// locked | hidden | open | progress | done
    pub state: String,
    /// Progress from 0 to 1.
    pub progress: f32,
    /// point | dwell | area | line | courier | roundtrip | collect | cells | steps | away
    pub shape: String,
    /// Where the quest is on the map, if it has a place.
    pub anchor: Option<GeoPoint>,
    /// The second place of a two-place quest (the courier drop-off).
    pub anchor_b: Option<GeoPoint>,
    /// Radius or corridor width of the target, in metres.
    pub radius_m: f64,
    /// The path to follow, or the area outline.
    pub path: Vec<GeoPoint>,
    /// One-line description of what to do.
    pub detail: String,
    /// True when a street quest stands in for a family the realm could not offer.
    pub fallback: bool,
    /// Whether this is the realm's boss quest.
    pub boss: bool,
    /// Short description.
    pub blurb: String,
    /// Solo only: what the quest gave, once done.
    pub reward: Option<String>,
    /// Id of the progressive chain this quest is a milestone of, if any.
    pub chain_id: Option<String>,
    /// A forager quest's items and counts; `None` for other quests.
    pub collect: Option<CollectOut>,
    /// The card's one-line goal ("Get here", "Sit 5 min").
    pub headline: String,
    /// The card's measurements under the headline ("Within 130 ft"); `detail` stays the full goal text.
    pub card_detail: String,
    /// The card's place row, for quests on a real place.
    pub place_row: Option<PlaceOut>,
    /// The card's labelled progress block, for quests that measure progress.
    pub progress_block: Option<ProgressOut>,
    /// Whether `Engine::about` has an about sheet for the quest's place (the card's "About this place").
    pub has_about: bool,
}

/// A quest card's place row: the place's name (if it has one) and what it is.
#[derive(Debug, uniffi::Record)]
pub struct PlaceOut {
    /// Name of the place; `None` for an unnamed one.
    pub name: Option<String>,
    /// What the place is ("Bench", "Artwork").
    pub kind: Option<String>,
    /// Whether the name is a title shown in quotes (an artwork's).
    pub quoted: bool,
}

/// A quest card's progress block.
#[derive(Debug, uniffi::Record)]
pub struct ProgressOut {
    /// What is measured ("Time here", "Traveled").
    pub label: String,
    /// How far along, in words ("1:12 of 3:00").
    pub value: String,
    /// How far along, from 0 to 1.
    pub frac: f32,
    /// A line under the bar ("Carrying 2").
    pub hint: Option<String>,
    /// Named steps of a two-step quest; empty for others.
    pub steps: Vec<String>,
    /// Index of the current step.
    pub step_at: u8,
}

/// One row of a place's about sheet.
#[derive(Debug, uniffi::Record)]
pub struct AboutRowOut {
    /// Row label ("Artist").
    pub label: String,
    /// Row value.
    pub value: String,
}

/// What the map knows about a quest's place, beyond its name and type.
#[derive(Debug, uniffi::Record)]
pub struct AboutOut {
    /// Free-text description.
    pub description: Option<String>,
    /// Labelled facts, in a fixed order.
    pub rows: Vec<AboutRowOut>,
    /// Wikipedia article address.
    pub wikipedia_url: Option<String>,
}

/// One item of a forager quest.
#[derive(Debug, uniffi::Record)]
pub struct CollectItemOut {
    /// Where the item lies.
    pub at: GeoPoint,
    /// Whether the player has picked it up.
    pub picked: bool,
}

/// A forager quest's items and how far it has got.
#[derive(Debug, uniffi::Record)]
pub struct CollectOut {
    /// What the items are ("pinecones").
    pub theme: String,
    /// How many must be brought home.
    pub need: u32,
    /// Picked up and not yet brought home.
    pub carried: u32,
    /// Brought home so far.
    pub banked: u32,
    /// The items still on the map (picked ones are gone; new ones appear as the map tops up).
    pub items: Vec<CollectItemOut>,
    /// Whether the map draws these items instead of the quest's pin (see `collect_out`).
    pub on_map: bool,
}

/// The shared backpack: what every forager carries (#142).
#[derive(Debug, uniffi::Record)]
pub struct BagOut {
    /// Items it holds before steps slow down.
    pub size: u32,
    /// Each carried item's theme ("acorns"), for its icon, grouped by quest.
    pub items: Vec<String>,
    /// Items carried past `size`.
    pub over: u32,
    /// How much a step counts now (1 when not over).
    pub step_factor: f32,
    /// Empty, Light, Full or "Heavy · steps −N%".
    pub load: String,
}

/// A forager's items and counts as the app shows them in `state`; `None` for other quests. The map draws the items instead of the pin
/// (`on_map`) while it is unfinished, not fogged and items are still out there; a done one, or one with every item picked (its pin
/// then sits at `pin_at`), is drawn like any quest.
fn collect_out(t: &Target, c: Option<&Collected>, live: &[Point], state: QuestState) -> Option<CollectOut> {
    let Target::Collect { need, theme, .. } = t else { return None };
    let c = c.cloned().unwrap_or_default();
    // `picked` stays in the record for the map's filters; live items are never picked.
    let items = live.iter().map(|p| CollectItemOut { at: gp(*p), picked: false }).collect();
    Some(CollectOut {
        theme: theme.clone(),
        need: *need,
        carried: c.carried,
        banked: c.banked,
        items,
        on_map: !matches!(state, QuestState::Done | QuestState::Hidden) && !live.is_empty(),
    })
}

/// The first item still out there, where the quest's pin and popup sit.
fn first_open(c: &CollectOut) -> Option<Point> {
    c.items.iter().find(|i| !i.picked).map(|i| pt(&i.at))
}

/// Where a quest's pin and popup sit: an unfinished forager's first item still out there, or `home` once every item is picked and some
/// are still carried (bring them home); otherwise its own anchor.
fn pin_at(collect: Option<&CollectOut>, state: QuestState, anchor: Option<Point>, home: Point) -> Option<Point> {
    match collect {
        Some(c) if state != QuestState::Done => first_open(c).or_else(|| (c.carried > 0).then_some(home)).or(anchor),
        _ => anchor,
    }
}

/// A zone of the open game.
#[derive(Debug, uniffi::Record)]
pub struct ZoneOut {
    /// Zone number.
    pub id: u32,
    /// Whether the player can enter the zone.
    pub unlocked: bool,
    /// Zone keys needed to open the zone.
    pub keys_needed: u32,
    /// Name of the realm the zone is played in.
    pub realm_name: String,
    /// The realm this zone is played in (the Play map shows only the realms of the open game).
    pub realm_id: String,
}

/// Everything the Play screen shows besides the quest list.
#[derive(Debug, uniffi::Record)]
pub struct HudOut {
    /// Whether time away is running, so its live value moves with the clock (the Play screen redraws it now and then while shown).
    pub away_running: bool,
    /// Each goal with its own progress (one entry for a single-goal game).
    pub goals: Vec<GoalLineOut>,
    /// Short text describing the win condition.
    pub goal_label: String,
    /// Progress toward the win condition, 0 to 1.
    pub goal_progress: f32,
    /// Whether the win condition is met.
    pub goal_achieved: bool,
    /// Quests completed.
    pub done: u32,
    /// Quests in all.
    pub total: u32,
    /// Zone keys held.
    pub keys: u32,
    /// Tool items held.
    pub tools: Vec<String>,
    /// The letters collected so far for a letter-hunt goal.
    pub letters: String,
    /// Labels of the active traps.
    pub traps: Vec<String>,
    /// Where to go to thaw a freeze trap, if one is active.
    pub thaw: Option<GeoPoint>,
    /// The detour waypoint still to be visited, if any.
    pub waypoint: Option<GeoPoint>,
    /// Why checks are blocked right now (a trap), if they are.
    pub blocked: Option<String>,
    /// The active traps and the block as one line, each once, for the part of the Play panel that never hides; `None` with neither.
    pub danger: Option<String>,
    /// Distance travelled so far, in kilometres.
    pub distance_km: f64,
    /// Days in a row on which a quest was completed.
    pub streak_days: u32,
    /// Whether fog of war is on.
    pub fog: bool,
    /// `solo` or `archipelago`.
    pub backend: String,
    /// Display name of the game.
    pub game_name: String,
}

/// The walk recap ("Welcome home"), see `apgo_core::recap::Recap`.
#[derive(Debug, uniffi::Record)]
pub struct RecapOut {
    /// When the player left home, Unix ms.
    pub start_ms: i64,
    /// When they arrived home, Unix ms.
    pub end_ms: i64,
    /// Raw steps walked.
    pub steps: i64,
    /// Length of the recorded trace in metres.
    pub distance_m: f64,
    /// Map squares seen for the first time.
    pub new_cells: u32,
    /// Names of the quests completed.
    pub quests_done: Vec<String>,
    /// Checks sent to the server.
    pub checks_sent: u32,
    /// Items received.
    pub items: Vec<String>,
    /// Zones unlocked.
    pub zones_unlocked: Vec<u32>,
    /// Forager items brought home.
    pub banked: u32,
    /// The thinned trace, one line per recorded stretch.
    pub route: Vec<Vec<GeoPoint>>,
}

impl From<&Recap> for RecapOut {
    fn from(r: &Recap) -> Self {
        Self {
            start_ms: r.start_ms,
            end_ms: r.end_ms,
            steps: r.steps,
            distance_m: r.distance_m,
            new_cells: r.new_cells,
            quests_done: r.quests_done.clone(),
            checks_sent: r.checks_sent,
            items: r.items.clone(),
            zones_unlocked: r.zones_unlocked.clone(),
            banked: r.banked,
            route: r.route.iter().map(|s| s.iter().map(|p| GeoPoint { lat: p.lat, lon: p.lon }).collect()).collect(),
        }
    }
}

/// Something that happened that the host should react to.
#[derive(Debug, uniffi::Enum)]
pub enum EventOut {
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
        /// Whether to alert the player with a loud notification and buzz: trap alerts are on and no Silence Trap mutes them.
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

/// `traps` and `blocked` as one line, each once; `None` when there is neither.
fn danger_line(traps: &[String], blocked: Option<String>) -> Option<String> {
    let mut all: Vec<String> = Vec::new();
    for t in traps.iter().cloned().chain(blocked) {
        if !all.contains(&t) {
            all.push(t);
        }
    }
    (!all.is_empty()).then(|| all.join("  ·  "))
}

/// `e` for the app; a trap alerts only while `alerts` (the player's trap alerts setting) is on.
fn ev_out(e: Event, alerts: bool) -> EventOut {
    match e {
        Event::QuestDone { location_id, name } => EventOut::QuestDone { location_id, name },
        Event::SendCheck { location_id } => EventOut::SendCheck { location_id },
        Event::Reward { location_id, item } => EventOut::Reward { location_id, item },
        Event::ZoneUnlocked { zone } => EventOut::ZoneUnlocked { zone },
        Event::Trap { item, message, alert } => EventOut::Trap { item, message, alert: alert && alerts },
        Event::ShuffleRequested => EventOut::ShuffleRequested,
        Event::Discovered { location_id } => EventOut::Discovered { location_id },
        Event::GoalAchieved { label } => EventOut::GoalAchieved { label },
        Event::Info { text } => EventOut::Info { text },
    }
}

fn describe(t: &Target, units: UnitSystem) -> (&'static str, Option<Point>, Option<Point>, f64, Vec<Point>, String) {
    let text = t.goal_text(units);
    match t {
        Target::Point { p, r } => ("point", Some(*p), None, *r, vec![], text),
        Target::Dwell { p, r, .. } => ("dwell", Some(*p), None, *r, vec![], text),
        Target::DwellArea { poly, center, r, .. } => ("area", Some(*center), None, *r, poly.clone(), text),
        Target::Line { pts, corridor_m, .. } => ("line", pts.first().copied(), None, *corridor_m, pts.clone(), text),
        Target::Courier { a, b, r, .. } => ("courier", Some(*a), Some(*b), *r, vec![], text),
        Target::RoundTrip { far, r, .. } => ("roundtrip", Some(*far), None, *r, vec![], text),
        Target::Cells { cell_m, .. } => ("cells", None, None, *cell_m, vec![], text),
        Target::Steps { .. } => ("steps", None, None, 0.0, vec![], text),
        Target::Away { .. } => ("away", None, None, 0.0, vec![], text),
        Target::Collect { pts, r, .. } => ("collect", pts.first().copied(), None, *r, vec![], text),
    }
}

/// One unbroken stretch of the trace (the line breaks where the phone was off or the app closed).
#[derive(Debug, uniffi::Record)]
pub struct TrackSegmentOut {
    /// The points of the segment, in order.
    pub points: Vec<GeoPoint>,
}

/// The current session's display line.
#[derive(Debug, uniffi::Record)]
pub struct TraceOut {
    /// When it starts, Unix ms (the journal's points before this are the older trace).
    pub from_ms: Option<i64>,
    /// Its lines, oldest first: it breaks where the location filter restarted far away.
    pub runs: Vec<TrackSegmentOut>,
}

/// What changed in the current session's display line since a cursor (see `Engine::trace_matched_since`).
#[derive(Debug, uniffi::Record)]
pub struct TraceDelta {
    /// Replace everything held with `append` (the cursor was 0, stale or from another game, or the line was thinned at its memory cap).
    pub reset: bool,
    /// Pass this next time; it only grows.
    pub cursor: u64,
    /// When the line starts, Unix ms (the journal's points before this are the older trace).
    pub from_ms: Option<i64>,
    /// The first of `append` carries on the last line held; every other one starts a new line.
    pub joins: bool,
    /// Settled points added since the cursor, as lines, oldest first.
    pub append: Vec<TrackSegmentOut>,
    /// The provisional end of the line, whole: replace the previous one (it starts at the newest settled point when it joins it).
    pub tail: TrackSegmentOut,
}

/// One line of the activity log.
#[derive(Debug, uniffi::Record)]
pub struct AuditEventOut {
    /// When it happened, in Unix milliseconds.
    pub t_ms: i64,
    /// Machine name of the event kind.
    pub kind: String,
    /// Human-readable detail.
    pub detail: String,
    /// Where the player was when it happened, if known.
    pub at: Option<GeoPoint>,
}

/// The seed name an Archipelago game was given before games remembered their server's `seed/slot`.
const LEGACY_AP_SEED: &str = "archipelago";

/// Where the player is with respect to home (`Engine::presence_state`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum PresenceHome {
    /// Home: GPS off, nothing counts.
    Home,
    /// Telling with GPS whether a walk or a Wi-Fi drop left the home circle: nothing counts yet.
    Checking,
    /// Away from home.
    Away,
}

/// Whether the open game belongs to the connected Archipelago server ([`Engine::ap_server_match`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum ServerMatch {
    /// Started on this server's seed and slot.
    Same,
    /// Started on another seed or slot: exchange nothing.
    Other,
    /// Not known (a game from before, or none open).
    Unknown,
}

/// What to tell the Archipelago server on (re)connect ([`Engine::server_report`]).
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct ServerReportOut {
    /// Every finished location id, ascending.
    pub checks: Vec<i64>,
    /// Whether the goal was reached.
    pub goal: bool,
}

/// Where an Archipelago game connects ([`Engine::ap_saved_server`]).
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct ApServerOut {
    /// Server address as typed (`host:port`, with or without a scheme).
    pub address: String,
    /// Slot name.
    pub slot: String,
}

/// Quests this close to a fix get a line in the log saying whether they count.
const NEAR_MISS_RADIUS_M: f64 = 100.0;
/// Longest the open game goes unsaved while the player moves without lasting events (each save rewrites and syncs the whole file;
/// the app also saves when it leaves the screen).
const SAVE_INTERVAL_MS: i64 = 90_000;
/// The drawn trace drops fixes within this of the last kept one (standing still), in metres.
const TRACE_MIN_STEP_M: f64 = 8.0;
/// The drawn trace smooths out wobble smaller than this, in metres.
const TRACE_TOLERANCE_M: f64 = 4.0;

/// A position fix from the phone, every field the filter can use (`None` = not reported; iOS sends `None` for its negative "unknown").
#[derive(Debug, Clone, uniffi::Record)]
pub struct FixIn {
    /// When the fix was taken, Unix ms (the fix's own clock).
    pub t_ms: i64,
    /// Latitude, degrees.
    pub lat: f64,
    /// Longitude, degrees.
    pub lon: f64,
    /// 68 % horizontal radius, metres.
    pub accuracy_m: f64,
    /// Ground speed, m/s.
    pub speed_mps: Option<f64>,
    /// 68 % speed accuracy, m/s.
    pub speed_acc_mps: Option<f64>,
    /// Course, degrees from north.
    pub bearing_deg: Option<f64>,
    /// 68 % course accuracy, degrees.
    pub bearing_acc_deg: Option<f64>,
    /// Altitude, metres.
    pub altitude_m: Option<f64>,
    /// 68 % vertical accuracy, metres.
    pub vertical_acc_m: Option<f64>,
    /// `fused`, `gps`, `network`, `ios`; anything else is "other".
    pub provider: String,
    /// Made by a mock-location app (and not allowed by the debug bench setting).
    pub mock: bool,
}

/// A step calibration the app saved (preferences `stepcal`, one entry per source).
#[derive(Debug, Clone, uniffi::Record)]
pub struct StepCalIn {
    /// Step source id, e.g. `phone.step_counter`.
    pub source: String,
    /// Scale on the cadence model.
    pub k: f64,
    /// Variance of the scale.
    pub var_k: f64,
    /// Windows learned from.
    pub samples: u32,
    /// Last update, Unix ms.
    pub updated_ms: i64,
}

/// The step calibration to save (same fields as [`StepCalIn`]).
#[derive(Debug, Clone, uniffi::Record)]
pub struct StepCalOut {
    /// Step source id.
    pub source: String,
    /// Scale on the cadence model.
    pub k: f64,
    /// Variance of the scale.
    pub var_k: f64,
    /// Windows learned from.
    pub samples: u32,
    /// Last update, Unix ms.
    pub updated_ms: i64,
}

fn step_cal_in(c: StepCalIn) -> StepCal {
    StepCal { source: c.source, k: c.k, var_k: c.var_k, samples: c.samples, updated_ms: c.updated_ms }
}

/// The calibration to save, `None` while nothing was learned: a fresh default must never overwrite a stored value (review M5; also
/// covers a save that runs before the stored value was loaded).
fn step_cal_to_save(c: StepCal) -> Option<StepCalOut> {
    (c.samples > 0).then_some(StepCalOut { source: c.source, k: c.k, var_k: c.var_k, samples: c.samples, updated_ms: c.updated_ms })
}

/// One compass reading (azimuth already corrected to true north by the phone).
#[derive(Debug, Clone, uniffi::Record)]
pub struct HeadingIn {
    /// Degrees from true north.
    pub azimuth_deg: f64,
    /// `high`, `medium`, `low` or `unreliable`.
    pub accuracy: String,
    /// Pitch, degrees.
    pub pitch_deg: f64,
    /// Roll, degrees.
    pub roll_deg: f64,
    /// When it was read: the sensor event time on the fix clock, Unix ms.
    pub t_ms: i64,
    /// The phone's own heading error (Google's fused orientation `headingErrorDegrees`, a 95 % half cone), degrees; `None` from the
    /// plain rotation vector, which only has `accuracy`.
    pub error_deg: Option<f64>,
}

/// What the map shows for the player (see `apgo_core::loc::DisplayPosition`).
#[derive(Debug, Clone, uniffi::Record)]
pub struct PositionOut {
    /// Shown latitude.
    pub lat: f64,
    /// Shown longitude.
    pub lon: f64,
    /// Estimate latitude.
    pub est_lat: f64,
    /// Estimate longitude.
    pub est_lon: f64,
    /// 68 % radius, metres.
    pub uncertainty_m: f64,
    /// Speed, m/s.
    pub speed_mps: f64,
    /// Course, degrees.
    pub course_deg: Option<f64>,
    /// Arrow direction, degrees.
    pub heading_deg: Option<f64>,
    /// `course`, `compass` or `none`.
    pub heading_source: String,
    /// Pin on a matched street.
    pub matched: bool,
    /// Matching confidence, 0..1.
    pub match_confidence: f64,
    /// `gps`, `bridged`, `predicted` or `stale`.
    pub source: String,
    /// Age of the estimate, ms.
    pub age_ms: i64,
    /// Jump instead of gliding: the newest fix restarted the filter (reset, relocation, simulated fix). A level, not an event: it stays
    /// true for every `position()` call until the next fix, so the app snaps once per new fix (compare the estimate's position or age).
    pub snap: bool,
    /// The shown position comes from the gap bridge, also once it ages to `predicted` or `stale`: never judge a zone by it.
    pub bridged_origin: bool,
}

fn heading_in(h: &HeadingIn) -> apgo_core::loc::HeadingIn {
    apgo_core::loc::HeadingIn {
        t_ms: h.t_ms,
        azimuth_deg: h.azimuth_deg,
        accuracy: apgo_core::loc::CompassAccuracy::parse(&h.accuracy),
        pitch_deg: h.pitch_deg,
        roll_deg: h.roll_deg,
        error_deg: h.error_deg,
    }
}

fn position_out(d: &DisplayPosition) -> PositionOut {
    PositionOut {
        lat: d.lat,
        lon: d.lon,
        est_lat: d.est_lat,
        est_lon: d.est_lon,
        uncertainty_m: d.uncertainty_m,
        speed_mps: d.speed_mps,
        course_deg: d.course_deg,
        heading_deg: d.heading_deg,
        heading_source: match d.heading_source {
            HeadingSource::Course => "course",
            HeadingSource::Compass => "compass",
            HeadingSource::None => "none",
        }
        .into(),
        matched: d.matched,
        match_confidence: d.match_confidence,
        source: match d.source {
            DisplaySource::Gps => "gps",
            DisplaySource::Bridged => "bridged",
            DisplaySource::Predicted => "predicted",
            DisplaySource::Stale => "stale",
        }
        .into(),
        age_ms: d.age_ms,
        snap: d.snap,
        bridged_origin: d.bridged_origin,
    }
}

/// Whether the host's `simulated` flag is honoured: only in a debug build of the core (adversarial review I3). Compile time, so a
/// release library can never be told to believe a fix, whatever the host (Android, the planned iOS client) passes.
const SIM_ALLOWED: bool = cfg!(debug_assertions);

/// The core's fix for `f`. Only `simulated` (where `sim_allowed`) makes a [`Provider::Sim`] fix: a provider named "sim" is
/// [`Provider::Other`], since a sim fix skips the mock, accuracy and time checks (adversarial review I3).
fn raw_fix(f: &FixIn, simulated: bool, sim_allowed: bool) -> RawFix {
    let provider = match Provider::parse(&f.provider) {
        _ if simulated && sim_allowed => Provider::Sim,
        Provider::Sim => Provider::Other,
        p => p,
    };
    RawFix {
        t_ms: f.t_ms,
        lat: f.lat,
        lon: f.lon,
        accuracy_m: f.accuracy_m,
        speed_mps: f.speed_mps,
        speed_acc_mps: f.speed_acc_mps,
        bearing_deg: f.bearing_deg,
        bearing_acc_deg: f.bearing_acc_deg,
        altitude_m: f.altitude_m,
        vertical_acc_m: f.vertical_acc_m,
        provider,
        mock: f.mock,
    }
}

/// Whether an accepted estimate goes into the journal: 5 s or 5 m after the last point written, or at once when the clock jumped (a switch
/// between simulated and real fixes, or a time before the last point: the simulator's clock runs ahead).
fn journal_due(last: Option<&TrackPoint>, p: &TrackPoint) -> bool {
    last.is_none_or(|l| {
        l.simulated != p.simulated || p.t_ms < l.t_ms || p.t_ms - l.t_ms >= 5_000 || distance_m(Point::new(l.lat, l.lon), Point::new(p.lat, p.lon)) >= 5.0
    })
}

/// What the engine keeps of the open game's zone realms, so per-fix checks and `zones()` read no realm file.
#[derive(Default)]
struct ZoneCache {
    /// Each zone's realm shape; zones whose realm is gone are left out.
    shapes: Vec<Shape>,
    /// Each zone realm's name, one per zone realm of the game (empty for a realm that is gone).
    names: Vec<String>,
}

/// The zone cache of `game` from its zone realms as saved now (`realms`, one per zone realm, `None` for one that is gone).
fn zone_cache_of(game: &Game, realms: Vec<Option<Realm>>) -> ZoneCache {
    let names = realms.iter().map(|r| r.as_ref().map(|r| r.name.clone()).unwrap_or_default()).collect();
    let shapes = realms.into_iter().take(game.slot.zones.len()).flatten().map(|r| r.shape).collect();
    ZoneCache { shapes, names }
}

/// The `fix_rejected` line for an estimate that may not count, if it is one of those.
fn rejected_detail(e: &Estimate, units: UnitSystem) -> Option<String> {
    if e.uncertain() {
        return Some(format!("uncertain {}", distance_rounded(e.uncertainty_m, units, Round::Up)));
    }
    match e.verdict {
        Verdict::Gated => Some("ignored as a GPS jump".into()),
        Verdict::Unusable => Some("unusable fix".into()),
        _ => None,
    }
}

/// The game engine: realms, scanning, game setup and play. One per app, shared by all screens.
#[derive(uniffi::Object)]
pub struct Engine {
    dir: PathBuf,
    /// Track and audit log; `None` if the file could not be opened (the game still plays, nothing is recorded).
    journal: Option<Mutex<Journal>>,
    /// The game that was open most recently (so the activity of a paused game can still be read).
    last_game: Mutex<Option<String>>,
    /// Messages from the core for the app's diagnostics log (stderr is lost on Android). Capped; drained by `take_diag`.
    diag: Mutex<Vec<String>>,
    /// Per quest: the last near-miss reason logged and when, so a minute standing at a target is a few lines, not hundreds.
    near_logged: Mutex<HashMap<i64, (String, i64)>>,
    /// When a rejected fix was last logged, so a bad-signal stretch is one line, not thousands.
    last_reject_log_ms: std::sync::atomic::AtomicI64,
    catalog: Catalog,
    game: Mutex<Option<Game>>,
    /// When the open game is next written to disk between events (so counters survive a kill).
    save_policy: Mutex<SavePolicy>,
    /// Shape and travel mode of each zone of the open game (the "inside a zone" check and the filter's mode at the player's position),
    /// and its realm names.
    zones: Mutex<ZoneCache>,
    /// The fix spacing the phone last said it asks for ([`Self::set_fix_spacing_hint`]), handed to every game opened.
    fix_spacing_hint: Mutex<Option<i64>>,
    // Where the last fix's estimate was relative to the zones, worked out once in `on_fix` for presence to read.
    last_proximity: Mutex<Option<Proximity>>,
    /// The phone's region (ISO country code) that `Auto` units follow; empty until the app sets it.
    region: Mutex<String>,
    /// The units text is written in, worked out from the unit setting and `region` whenever either changes.
    units: Mutex<UnitSystem>,
    /// The trap alerts setting, kept in memory so a trap never reads the settings file (saved by `set_trap_alerts`).
    pub(crate) trap_alerts: std::sync::atomic::AtomicBool,
    /// The last journal point written, for the 5 s / 5 m throttle.
    last_journal: Mutex<Option<TrackPoint>>,
    /// The last street-build generation handed out (see [`Game::set_streets`]).
    streets_gen: AtomicU64,
    /// Bumped by every realm save or delete, so a street swap that read the realm files before an edit does not undo it.
    realm_edits: AtomicU64,
    /// Stop flags of the scans running now (see [`Engine::cancel_scan`]).
    scans: ScanRegistry,
    /// Held while a finished scan saves and while a realm is deleted, so a delete can never land between a scan's last check
    /// that its realm exists and its save (which would bring the realm back).
    store_write: Mutex<()>,
    /// Whether the player is home (home Wi-Fi, the circle round the home pin), fed with every real fix and step reading.
    home_watch: Mutex<HomeWatch>,
}

/// The stop flags of running scans, by realm id. Each scan has its own flag, so two scans of one realm never clear each other's.
#[derive(Default)]
struct ScanRegistry(Mutex<HashMap<String, Vec<Arc<std::sync::atomic::AtomicBool>>>>);

impl ScanRegistry {
    fn lock(&self) -> MutexGuard<'_, HashMap<String, Vec<Arc<std::sync::atomic::AtomicBool>>>> {
        self.0.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// A new scan of `id`: its stop flag.
    fn begin(&self, id: &str) -> Arc<std::sync::atomic::AtomicBool> {
        let flag = Arc::new(std::sync::atomic::AtomicBool::new(false));
        self.lock().entry(id.to_owned()).or_default().push(Arc::clone(&flag));
        flag
    }

    /// The scan with `flag` is over: forget its flag only.
    fn end(&self, id: &str, flag: &Arc<std::sync::atomic::AtomicBool>) {
        let mut map = self.lock();
        if let Some(flags) = map.get_mut(id) {
            flags.retain(|f| !Arc::ptr_eq(f, flag));
            if flags.is_empty() {
                map.remove(id);
            }
        }
    }

    /// Stop every running scan of `id`.
    fn cancel(&self, id: &str) {
        for f in self.lock().get(id).into_iter().flatten() {
            f.store(true, std::sync::atomic::Ordering::SeqCst);
        }
    }
}

impl Engine {
    /// [`Engine::scan_realm`] with the fetcher and pacing injected. The scan stops early, making no more requests and saving
    /// nothing, when [`Engine::cancel_scan`] or a delete stops it, or when `keep_going` says the app no longer wants it.
    fn scan_realm_with(
        &self,
        id: &str,
        now_ms: u64,
        progress: &(dyn Fn(usize, usize) + Sync),
        keep_going: &(dyn Fn() -> bool + Sync),
        fetch: Fetch<'_>,
        pacing: &Pacing,
    ) -> Result<Vec<OfferOut>, CoreError> {
        use std::sync::atomic::Ordering;
        let store = self.store();
        let realm = store.get(id).ok_or_else(|| err("realm not found"))?;
        let flag = self.scans.begin(id);
        let stopped = || flag.load(Ordering::SeqCst) || !keep_going();
        // A delete between reading the realm and registering the flag had no flag to stop: look again now that it is in.
        let scanned = if store.get(id).is_none() {
            Err(apgo_core::overpass::Error::Stopped)
        } else {
            scan_with_stop(&realm, &self.catalog, now_ms, fetch, pacing, std::time::Instant::now() + SCAN_BUDGET, progress, &stopped)
        };
        let atlas = scanned.map_err(|e| {
            self.scans.end(id, &flag);
            err(e)
        })?;
        // Save under the write lock, so a delete either comes first (the realm is gone and nothing is saved) or waits until after.
        let saved = {
            let _write = self.store_write.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            let current = if stopped() { None } else { store.get(id) };
            current.map(|mut current| -> Result<_, CoreError> {
                store.save_atlas(&atlas).map_err(err)?;
                // The realm may have been edited while the scan ran: stamp the scan time on its current state.
                current.scanned_at_ms = Some(now_ms);
                store.save(&current).map_err(err)?;
                Ok(current)
            })
        };
        self.scans.end(id, &flag);
        let current = saved.ok_or_else(|| err("scan stopped: the realm was deleted or is no longer wanted"))??;
        let mut zoned = atlas;
        zoned.restrict_to(&current.shape.to_zone());
        zoned.apply_marks(&store.marks(id));
        Ok(self.offers_of(&zoned))
    }

    /// The directory the engine keeps its files in.
    pub(crate) fn dir(&self) -> &std::path::Path {
        &self.dir
    }

    /// The units text is written in now.
    pub(crate) fn unit_system(&self) -> UnitSystem {
        *self.units.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Work the units out again from the saved choice and the region (replaced by `region` when given), and switch the open
    /// game's text to them.
    pub(crate) fn refresh_units(&self, region: Option<String>) {
        let mut r = self.region.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(new) = region {
            *r = new;
        }
        let u = resolve_units(Settings::load(&self.dir).units, &r);
        *self.units.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = u;
        // Keep `region` locked until the game has the units too, so two racing refreshes cannot leave the game on the older
        // units while `units` holds the newer ones. No path takes `region` while holding the game lock.
        self.with_game(|g| g.set_units(u));
        drop(r);
    }

    // The saved home pin is the home circle's centre.
    fn with_home_center(self: Arc<Self>) -> Arc<Self> {
        self.watch().set_center(self.store().home());
        self
    }

    fn watch(&self) -> MutexGuard<'_, HomeWatch> {
        self.home_watch.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn store(&self) -> RealmStore {
        RealmStore::new(&self.dir)
    }

    fn cache(&self) -> PathBuf {
        self.dir.join("http-cache")
    }

    /// `ev` for the app, with each trap's alert following the player's trap alerts setting.
    fn events_out(&self, ev: Vec<Event>) -> Vec<EventOut> {
        let alerts = self.trap_alerts.load(std::sync::atomic::Ordering::Relaxed);
        ev.into_iter().map(|e| ev_out(e, alerts)).collect()
    }

    /// Run `f` on the journal if there is one; failures are reported and never interrupt play.
    fn journal_do(&self, f: impl FnOnce(&Journal) -> rusqlite::Result<()>) {
        if let Some(j) = &self.journal {
            if let Err(e) = f(&j.lock().unwrap_or_else(std::sync::PoisonError::into_inner)) {
                self.note(format!("journal write failed: {e}"));
            }
        }
    }

    /// Of the quests the player is near, those worth a log line now: the reason changed, or 30 s passed since the last line for it.
    fn new_near_misses(&self, near: Vec<NearMiss>, t_ms: i64) -> Vec<NearMiss> {
        let mut seen = self.near_logged.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        near.into_iter()
            .filter(|n| {
                let fresh = seen.get(&n.location_id).is_none_or(|(reason, at)| *reason != n.reason || t_ms.saturating_sub(*at) >= 30_000);
                if fresh {
                    seen.insert(n.location_id, (n.reason.clone(), t_ms));
                }
                fresh
            })
            .collect()
    }

    /// The journal's trace segments of a game between two times; empty without a journal or when it cannot be read.
    fn read_segments(&self, game_id: &str, from_ms: i64, to_ms: i64) -> Vec<Vec<TrackPoint>> {
        let Some(j) = self.journal.as_ref() else { return Vec::new() };
        let read = j.lock().unwrap_or_else(std::sync::PoisonError::into_inner).segments(game_id, from_ms, to_ms, DEFAULT_MAX_GAP_MS);
        read.unwrap_or_else(|e| {
            self.note(format!("journal read failed: {e}"));
            Vec::new()
        })
    }

    /// Queue a message for the app's diagnostics log. Keeps the newest 200.
    fn note(&self, msg: String) {
        let mut q = self.diag.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        q.push(msg);
        if q.len() > 200 {
            q.remove(0);
        }
    }

    /// Save `g` when an event happened or the save interval has passed; a failure goes to the diagnostics log.
    fn save_if_due(&self, g: &Game, t_ms: i64, eventful: bool) {
        let due = self.save_policy.lock().unwrap_or_else(std::sync::PoisonError::into_inner).due(t_ms, eventful);
        if due {
            if let Err(e) = g.save(&self.dir) {
                self.note(format!("could not save game {}: {e}", g.id));
            }
        }
    }

    /// The current scans of `zone_realms` (each realm once), restricted to their shapes.
    fn zone_realm_atlases(&self, zone_realms: &[String]) -> Vec<(Realm, Atlas)> {
        let store = self.store();
        zone_realms.iter().collect::<BTreeSet<_>>().into_iter().filter_map(|id| store.get(id)).filter_map(|r| self.zoned_atlas(&r).map(|a| (r, a))).collect()
    }

    /// The open game's id and zone realms, if it plays in `realm_id` (any open game when `None`).
    fn streets_target(&self, realm_id: Option<&str>) -> Option<(String, Vec<String>)> {
        let slot = self.game.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        slot.as_ref().filter(|g| realm_id.is_none_or(|id| g.zone_realms.iter().any(|z| z == id))).map(|g| (g.id.clone(), g.zone_realms.clone()))
    }

    /// A new street-build generation; take it before reading the scans, so a later build always has a higher one.
    fn next_streets_gen(&self) -> u64 {
        self.streets_gen.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1
    }

    /// Swap built streets into the open game if it is still `game_id` over `zone_realms` and nothing newer went in; true when they did.
    /// The zone shapes are read again then too, so an outline edit moves in-zone, proximity and the filter's mode (final review M2).
    fn swap_streets(&self, game_id: &str, zone_realms: &[String], streets: Streets, generation: u64, realms: &[(Realm, Atlas)]) -> bool {
        let edits = self.realm_edits.load(std::sync::atomic::Ordering::SeqCst);
        let saved = self.zone_realm_files(zone_realms); // read before the lock: no file reads while fixes wait
        self.swap_streets_from(game_id, zone_realms, streets, generation, realms, saved, edits)
    }

    /// [`Self::swap_streets`] with the zone realm files as read (`saved`) when the realm edit count was `edits`.
    #[allow(clippy::too_many_arguments)] // split out so a test can land an edit between the file read and the lock
    fn swap_streets_from(
        &self,
        game_id: &str,
        zone_realms: &[String],
        streets: Streets,
        generation: u64,
        realms: &[(Realm, Atlas)],
        saved: Vec<Option<Realm>>,
        edits: u64,
    ) -> bool {
        let atlases: Vec<&Atlas> = realms.iter().map(|(_, a)| a).collect();
        let mut slot = self.game.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let Some(g) = slot.as_mut().filter(|g| g.id == game_id && g.zone_realms == zone_realms) else { return false };
        if !g.set_streets(streets, generation) {
            return false;
        }
        // A game saved before quests kept their place gets its places back from the same scans, once (then it is saved linked).
        // Forager items appear on the same scans' streets (#142): the first build fills each forager, later ones refresh its pool.
        g.set_forage_pools(realms);
        if g.relink_places(&atlases, &self.catalog) > 0 {
            if let Err(e) = g.save(&self.dir) {
                self.note(format!("could not save game {}: {e}", g.id));
            }
        }
        // Set while the game lock is held, so a game installed meanwhile cannot get these shapes (lock order: game, then zones).
        // A realm edit since the files were read already set fresher zones (refresh_zones_of): keep those.
        if self.realm_edits.load(std::sync::atomic::Ordering::SeqCst) == edits {
            let zones = zone_cache_of(g, saved);
            self.set_zones(slot, zones);
        }
        true
    }

    /// Each realm in `ids` as saved now (`None` for one that is gone).
    fn zone_realm_files(&self, ids: &[String]) -> Vec<Option<Realm>> {
        let store = self.store();
        ids.iter().map(|id| store.get(id)).collect()
    }

    /// Make `game` the open game and remember the shapes of its zones' realms (for "inside a zone" checks). Its streets come later,
    /// from [`Self::refresh_streets`].
    fn install(&self, mut game: Game) {
        *self.last_journal.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = None;
        self.save_policy.lock().unwrap_or_else(std::sync::PoisonError::into_inner).reset();
        // Only one game is played at a time; remember which, so closing the app without pausing resumes it on the next start.
        if let Err(e) = Game::mark_playing(&self.dir, &game.id) {
            self.note(format!("could not remember game {} as being played: {e}", game.id));
        }
        let realms = self.zone_realm_files(&game.zone_realms);
        let mut slot = self.game.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        // Read the units under the game lock: `refresh_units` stores new units before it takes this lock, so a change racing
        // this install is either seen here or applied to this game right after.
        game.set_units(self.unit_system());
        game.set_fix_spacing_hint(*self.fix_spacing_hint.lock().unwrap_or_else(std::sync::PoisonError::into_inner));
        // Keep the outgoing game's progress (unless the new one replaces that very save).
        if let Some(old) = slot.as_ref().filter(|old| old.id != game.id) {
            if let Err(e) = old.save(&self.dir) {
                self.note(format!("could not save game {} before replacing it: {e}", old.id));
            }
        }
        let zones = zone_cache_of(&game, realms);
        *slot = Some(game);
        self.set_zones(slot, zones);
    }

    /// Realm `id` was redrawn or deleted: when it is a zone of the open game, in-zone checks use what is saved now, not what was
    /// saved when the game was opened.
    fn refresh_zones_of(&self, id: &str) {
        self.realm_edits.fetch_add(1, std::sync::atomic::Ordering::SeqCst); // before the lock: a swap holding it sees the edit
        let slot = self.game.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let Some(zones) = slot.as_ref().filter(|g| g.zone_realms.iter().any(|r| r == id)).map(|g| zone_cache_of(g, self.zone_realm_files(&g.zone_realms)))
        else {
            return;
        };
        self.set_zones(slot, zones);
    }

    // The open game's zone shapes; a new set (another game, or none) forgets where the last fix was relative to the old ones.
    // Takes the game guard and releases it after, so the shapes only ever change together with the game, never between another
    // thread's read and write (lock order: game, then zones).
    fn set_zones(&self, game: MutexGuard<'_, Option<Game>>, zones: ZoneCache) {
        *self.zones.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = zones;
        *self.last_proximity.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = None;
        drop(game);
    }

    /// Distance in metres from a point to the nearest zone area of the open game (0 inside), or `None` with no game.
    fn zone_distance_m(&self, p: Point) -> Option<f64> {
        let zones = self.zones.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        zones.shapes.iter().map(|s| s.distance_m(p)).reduce(f64::min)
    }

    /// Remember where the last fix's estimate was relative to the open game's zones (`None`: no zones), for presence.
    fn set_proximity(&self, zone_d: Option<f64>) {
        *self.last_proximity.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = Proximity::of_distance(zone_d);
    }

    /// Whether `p` is the next journal point (5 s or 5 m after the last one); if so it becomes the last one.
    fn journal_point_due(&self, p: &TrackPoint) -> bool {
        let mut last = self.last_journal.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let due = journal_due(last.as_ref(), p);
        if due {
            *last = Some(p.clone());
        }
        due
    }

    /// Whether a rejected fix at `t_ms` gets a log line (one a minute, or at once when `t_ms` is before the last line: the simulator's clock
    /// runs ahead); if so the minute starts again.
    fn reject_log_due(&self, t_ms: i64) -> bool {
        let last_ms = self.last_reject_log_ms.load(std::sync::atomic::Ordering::Relaxed);
        let due = t_ms < last_ms || t_ms.saturating_sub(last_ms) >= 60_000;
        if due {
            self.last_reject_log_ms.store(t_ms, std::sync::atomic::Ordering::Relaxed);
        }
        due
    }

    fn game_id(&self) -> Option<String> {
        self.with_game(|g| g.id.clone())
    }

    fn with_game<T>(&self, f: impl FnOnce(&mut Game) -> T) -> Option<T> {
        self.game.lock().unwrap_or_else(std::sync::PoisonError::into_inner).as_mut().map(f)
    }

    /// The realm's scanned atlas, restricted to the realm's current zone (see `Atlas::restrict_to`).
    fn zoned_atlas(&self, realm: &Realm) -> Option<Atlas> {
        let mut a = self.store().load_atlas(&realm.id)?;
        a.restrict_to(&realm.shape.to_zone());
        Some(a)
    }

    /// Finds of a (zoned) atlas: place index -> the quest kinds it can serve for this realm's mode.
    fn kinds_by_place<'a>(&'a self, atlas: &Atlas) -> BTreeMap<usize, Vec<&'a Kind>> {
        let mut out: BTreeMap<usize, Vec<&Kind>> = BTreeMap::new();
        for (kind_id, idxs) in &atlas.matches {
            let Some(kind) = self.catalog.kind(kind_id) else { continue };
            for &i in idxs {
                out.entry(i).or_default().push(kind);
            }
        }
        out
    }

    fn realm_atlases(&self, ids: &[String]) -> Result<Vec<(Realm, Atlas)>, CoreError> {
        let store = self.store();
        ids.iter()
            .map(|id| {
                let r = store.get(id).ok_or_else(|| err(format!("realm {id} not found")))?;
                let mut a = self.zoned_atlas(&r).ok_or_else(|| err(format!("realm \"{}\" has not been scanned yet", r.name)))?;
                a.apply_marks(&store.marks(id)); // banned places are left out, favorites are preferred
                Ok((r, a))
            })
            .collect()
    }

    fn home_for(&self, realms: &[(Realm, Atlas)]) -> Point {
        self.store().home().or_else(|| realms.first().map(|(r, _)| r.shape.center())).unwrap_or(Point::new(0.0, 0.0))
    }

    fn offers_of(&self, atlas: &Atlas) -> Vec<OfferOut> {
        let mut v: Vec<OfferOut> = atlas
            .offers(&self.catalog, &Mode::ALL)
            .into_iter()
            .filter_map(|(id, count)| {
                self.catalog.kind(&id).map(|k| OfferOut { kind_id: id, name: k.name.clone(), family: k.family.clone(), blurb: k.blurb.clone(), count })
            })
            .collect();
        v.sort_by(|a, b| b.count.cmp(&a.count).then(a.name.cmp(&b.name)));
        v
    }
}

#[uniffi::export]
#[allow(clippy::needless_pass_by_value)] // uniffi requires owned args
impl Engine {
    /// An engine storing its files under `dir`.
    #[uniffi::constructor]
    pub fn new(dir: String) -> Arc<Self> {
        let dir = PathBuf::from(dir);
        let _ = std::fs::create_dir_all(&dir);
        let mut diag = Vec::new();
        let journal = Journal::open(&dir.join("journal.db")).map_err(|e| diag.push(format!("journal unavailable: {e}"))).ok().map(Mutex::new);
        Arc::new(Self {
            journal,
            diag: Mutex::new(diag),
            near_logged: Mutex::new(HashMap::default()),
            last_game: Mutex::new(None),
            last_reject_log_ms: std::sync::atomic::AtomicI64::new(i64::MIN),
            catalog: Catalog::builtin(),
            game: Mutex::new(None),
            save_policy: Mutex::new(SavePolicy::new(SAVE_INTERVAL_MS)),
            zones: Mutex::new(ZoneCache::default()),
            fix_spacing_hint: Mutex::new(None),
            last_proximity: Mutex::new(None),
            units: Mutex::new(resolve_units(Settings::load(&dir).units, "")),
            trap_alerts: std::sync::atomic::AtomicBool::new(Settings::load(&dir).trap_alerts),
            region: Mutex::new(String::new()),
            last_journal: Mutex::new(None),
            streets_gen: AtomicU64::default(),
            realm_edits: AtomicU64::default(),
            scans: ScanRegistry::default(),
            store_write: Mutex::new(()),
            home_watch: Mutex::new(HomeWatch::default()),
            dir,
        })
        .with_home_center()
    }

    /// Number of quest kinds in the catalog.
    pub fn catalog_size(&self) -> u32 {
        count_u32(self.catalog.kinds.len())
    }

    // ---------- realms ----------
    /// Every saved realm.
    pub fn realms(&self) -> Vec<RealmOut> {
        let store = self.store();
        store
            .list()
            .into_iter()
            .map(|r| {
                let circle = r.circle().map(|(center, radius_m)| CircleOut { center: gp(center), radius_m });
                let polygon: Vec<GeoPoint> = r.polygon().unwrap_or_default().iter().map(|p| gp(*p)).collect();
                let polygon_active = r.polygon_active();
                let atlas = store.load_atlas(&r.id);
                // Count finds (zoned, usable by this realm's mode), the same number the Details list shows.
                let places = self.zoned_atlas(&r).map_or(0, |a| count_u32(self.kinds_by_place(&a).len()));
                let warning = atlas.and_then(|a| a.warnings.first().cloned());
                RealmOut { id: r.id, name: r.name, icon: r.icon.clone(), circle, polygon, polygon_active, scanned_at_ms: r.scanned_at_ms, places, warning }
            })
            .collect()
    }

    /// Saves a realm with both outlines it has; `polygon_active` picks the real one, the other is kept in reserve.
    /// Save a realm with its outlines.
    ///
    /// # Errors
    /// Returns an error if the active outline is missing or the shape is not valid, or the file cannot be written.
    pub fn save_realm(
        &self,
        id: String,
        name: String,
        icon: Option<String>,
        circle: Option<CircleOut>,
        polygon: Vec<GeoPoint>,
        polygon_active: bool,
    ) -> Result<(), CoreError> {
        let circle = circle.map(|c| Shape::Circle { center: pt(&c.center), radius_m: c.radius_m });
        let polygon = (!polygon.is_empty()).then(|| Shape::Polygon { vertices: polygon.iter().map(pt).collect() });
        let (shape, spare) = match (polygon_active, circle, polygon) {
            (true, c, Some(p)) => (p, c),
            (false, Some(c), p) => (c, p),
            _ => return Err(err("the active outline is missing")),
        };
        let prev = self.store().get(&id);
        self.store().save(&Realm { id: id.clone(), name, icon, shape, spare, scanned_at_ms: prev.and_then(|p| p.scanned_at_ms) }).map_err(err)?;
        self.refresh_zones_of(&id);
        Ok(())
    }

    /// Build the open game's street index (trap targets) and street graph (location filter) from the current scans and swap them in:
    /// after a game is opened or started (which never wait for them: the game plays without a graph until this lands), and after a
    /// realm it plays in (`realm_id`) was edited or scanned again. The build runs outside the game lock, so fixes never wait for it;
    /// slow on a big realm, so call it off the main thread. `None` when there is nothing to do (no open game, it does not play in
    /// `realm_id`, or a newer build already landed).
    pub fn refresh_streets(&self, realm_id: Option<String>) -> Option<StreetsBuiltOut> {
        let (game_id, zone_realms) = self.streets_target(realm_id.as_deref())?;
        let generation = self.next_streets_gen();
        let started = std::time::Instant::now();
        let realms = self.zone_realm_atlases(&zone_realms);
        let atlases: Vec<&Atlas> = realms.iter().map(|(_, a)| a).collect();
        let streets = Game::build_streets(&atlases);
        let build_ms = u32::try_from(started.elapsed().as_millis()).unwrap_or(u32::MAX);
        let (segments, degraded) = streets.1.as_ref().map_or((0, false), |g| (count_u32(g.segment_count()), g.is_degraded()));
        self.swap_streets(&game_id, &zone_realms, streets, generation, &realms).then_some(StreetsBuiltOut { build_ms, segments, degraded })
    }

    /// Delete a realm with its scan and marks.
    ///
    /// # Errors
    /// Returns an error if a file cannot be removed.
    pub fn delete_realm(&self, id: String) -> Result<(), CoreError> {
        {
            let _write = self.store_write.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            self.scans.cancel(&id);
            self.store().delete(&id).map_err(err)?;
        }
        self.refresh_zones_of(&id);
        Ok(())
    }

    /// Measurements of a shape as drawn (no scan needed). `home` is where distances are measured from; the shape's own centre if none is set.
    pub fn shape_stats(&self, circle: Option<CircleOut>, polygon: Vec<GeoPoint>, polygon_active: bool, home: Option<GeoPoint>) -> ShapeStatsOut {
        let shape = match (polygon_active, circle) {
            (false, Some(c)) => Shape::Circle { center: pt(&c.center), radius_m: c.radius_m },
            _ => Shape::Polygon { vertices: polygon.iter().map(pt).collect() },
        };
        let from = home.map_or_else(|| shape.center(), |h| pt(&h));
        ShapeStatsOut { area_m2: shape.area_m2(), perimeter_m: shape.perimeter_m(), farthest_m: shape.farthest_m(from) }
    }

    /// For each of `finds`, whether it lies inside the outline being drawn (the editor shows only those); see
    /// `apgo_core::realm::inside_draft`. One call for all finds.
    pub fn inside_draft(&self, finds: Vec<GeoPoint>, circle: Option<CircleOut>, polygon: Vec<GeoPoint>, polygon_active: bool) -> Vec<bool> {
        let pts: Vec<Point> = finds.iter().map(pt).collect();
        let corners: Vec<Point> = polygon.iter().map(pt).collect();
        apgo_core::realm::inside_draft(&pts, circle.map(|c| (pt(&c.center), c.radius_m)), &corners, polygon_active)
    }

    /// What the scan found in the realm (inside its current shape), or none if it has not been scanned.
    pub fn realm_stats(&self, id: String) -> Option<RealmStatsOut> {
        let store = self.store();
        let realm = store.get(&id)?;
        let mut atlas = self.zoned_atlas(&realm)?;
        atlas.apply_marks(&store.marks(&id));
        let places = self.kinds_by_place(&atlas);
        let (mut trail_m, mut parks) = (0.0, 0u32);
        for (&i, kinds) in &places {
            if kinds.iter().any(|k| k.family == "trail") {
                trail_m += apgo_core::geo::polyline_len_m(&atlas.features[i].geometry);
            }
            if kinds.iter().any(|k| k.family == "park") {
                parks += 1;
            }
        }
        Some(RealmStatsOut {
            walkable_m: atlas.walkable_m(),
            rough_share: atlas.rough_share(),
            trail_m,
            parks,
            streets: count_u32(atlas.street_count()),
            finds: count_u32(places.len()),
            quest_types: count_u32(self.offers_of(&atlas).len()),
        })
    }

    /// What scanning a realm would cost right now: tiles and requests, and how many are not in the cache yet (those are the ones that go to the network).
    pub fn scan_plan(&self, id: String) -> ScanPlanOut {
        let Some(realm) = self.store().get(&id) else { return ScanPlanOut { tiles: 0, requests: 0, missing: 0 } };
        let cache = self.cache();
        let p = apgo_core::scan::plan(&realm.shape.to_zone(), &self.catalog, &|q| apgo_core::overpass::is_cached(q, &cache));
        ScanPlanOut { tiles: count_u32(p.tiles), requests: count_u32(p.jobs), missing: count_u32(p.missing()) }
    }

    /// Scan the realm over the network (through the shared tile cache) and save its atlas, telling `listener` as each request
    /// finishes. Returns what the realm offers.
    ///
    /// # Errors
    /// Returns an error if the realm does not exist, every map request failed, or the atlas cannot be saved.
    pub fn scan_realm(&self, id: String, now_ms: u64, listener: Arc<dyn ScanListener>) -> Result<Vec<OfferOut>, CoreError> {
        let cache = self.cache();
        let fetch = |q: &str, start: usize, deadline: Option<std::time::Instant>| apgo_core::overpass::fetch_cached_from(q, Some(&cache), start, deadline);
        let progress = |done, total| listener.progress(count_u32(done), count_u32(total));
        self.scan_realm_with(&id, now_ms, &progress, &|| listener.keep_going(), &fetch, &Pacing::default())
    }

    /// Stop the scan of realm `id` if one is running: it makes no more requests and saves nothing (its `scan_realm` returns an
    /// error). Nothing happens when the realm is not being scanned. Deleting a realm does this too.
    pub fn cancel_scan(&self, id: String) {
        self.scans.cancel(&id);
    }

    /// Whether realm `id` was scanned before street runs were recorded, so its quests are placed by street points only until it is
    /// scanned again (the app can rescan it quietly).
    pub fn realm_needs_rescan(&self, id: String) -> bool {
        self.store().load_atlas(&id).is_some_and(|a| a.needs_rescan())
    }

    /// The quest kinds realm `id` can offer, with counts.
    pub fn realm_offers(&self, id: String) -> Vec<OfferOut> {
        let store = self.store();
        match store.get(&id).and_then(|r| self.zoned_atlas(&r)) {
            Some(mut a) => {
                a.apply_marks(&store.marks(&id));
                self.offers_of(&a)
            }
            None => vec![],
        }
    }

    /// An evenly spread sample of at most `max` of a realm's usable finds (banned ones left out), for drawing a preview of the realm.
    pub fn realm_dots(&self, id: String, max: u32) -> Vec<DotOut> {
        let store = self.store();
        let Some(mut atlas) = store.get(&id).and_then(|r| self.zoned_atlas(&r)) else { return vec![] };
        atlas.apply_marks(&store.marks(&id));
        let places = self.kinds_by_place(&atlas);
        let stride = (places.len() / max.max(1) as usize).max(1);
        places
            .into_iter()
            .step_by(stride)
            .map(|(i, kinds)| DotOut { at: gp(atlas.features[i].point), kind_id: kinds[0].id.clone(), family: kinds[0].family.clone() })
            .collect()
    }

    /// Every find in a realm (a scanned spot that can serve a quest), with the player's mark on it, nearest first.
    pub fn realm_finds(&self, id: String) -> Vec<FindOut> {
        let store = self.store();
        let Some((realm, atlas)) = store.get(&id).and_then(|r| self.zoned_atlas(&r).map(|a| (r, a))) else { return vec![] };
        let marks = store.marks(&id);
        let home = store.home().unwrap_or_else(|| realm.shape.center());
        let kinds_of = self.kinds_by_place(&atlas);
        let units = self.unit_system();
        let mut out: Vec<FindOut> = kinds_of
            .into_iter()
            .map(|(i, kinds)| {
                let f = &atlas.features[i];
                FindOut {
                    id: f.id.clone(),
                    name: f.name.clone().unwrap_or_else(|| kinds[0].name.clone()),
                    named: f.name.is_some(),
                    kind_id: kinds[0].id.clone(),
                    family: kinds[0].family.clone(),
                    tags: {
                        let mut t: Vec<String> = kinds.iter().flat_map(|k| k.evidence(&f.tags)).collect();
                        t.sort();
                        t.dedup();
                        t
                    },
                    at: gp(f.point),
                    distance_m: distance_m(home, f.point),
                    mark: match marks.get(&f.id) {
                        Mark::None => "none",
                        Mark::Favorite => "favorite",
                        Mark::Banned => "banned",
                    }
                    .into(),
                    kinds: kinds
                        .into_iter()
                        .map(|k| KindOut { id: k.id.clone(), name: k.name.clone(), family: k.family.clone(), blurb: k.blurb.clone(), how: k.verify.how(units) })
                        .collect(),
                }
            })
            .collect();
        out.sort_by(|a, b| a.distance_m.total_cmp(&b.distance_m));
        out
    }

    /// `mark` is "none", "favorite" or "banned". Takes effect the next time quests are made or re-rolled.
    /// Mark a find as none, favorite or banned.
    ///
    /// # Errors
    /// Returns an error if the mark is unknown or the marks file cannot be written.
    pub fn set_find_mark(&self, realm_id: String, find_id: String, mark: String) -> Result<(), CoreError> {
        let mark = match mark.as_str() {
            "none" => Mark::None,
            "favorite" => Mark::Favorite,
            "banned" => Mark::Banned,
            other => return Err(err(format!("unknown mark {other}"))),
        };
        let store = self.store();
        let mut marks = store.marks(&realm_id);
        marks.set(&find_id, mark);
        store.save_marks(&realm_id, &marks).map_err(err)
    }

    /// The saved home point, if any.
    pub fn home(&self) -> Option<GeoPoint> {
        self.store().home().map(gp)
    }

    /// Save the home point.
    ///
    /// # Errors
    /// Returns an error if the file cannot be written.
    pub fn set_home(&self, p: GeoPoint) -> Result<(), CoreError> {
        self.store().set_home(pt(&p)).map_err(err)?;
        self.watch().set_center(Some(pt(&p)));
        Ok(())
    }

    // ---------- presence: home ----------
    /// Presence started (or restarted) at `t_ms`: decide home from the home Wi-Fi reading given so far and the home circle.
    pub fn presence_start(&self, t_ms: i64) {
        self.watch().start(t_ms);
    }

    /// The (debounced) home Wi-Fi reading: `Some(true)` on it, `Some(false)` on another network, `None` unknown or off.
    pub fn presence_wifi(&self, home: Option<bool>, t_ms: i64) {
        self.watch().on_wifi(home, t_ms);
    }

    /// The scheduled wake-up at [`Self::presence_due_ms`]: a GPS check that found nothing clear falls back.
    pub fn presence_tick(&self, t_ms: i64) {
        self.watch().tick(t_ms);
    }

    /// Where the player is with respect to home.
    pub fn presence_state(&self) -> PresenceHome {
        match self.watch().state() {
            HomeState::Home => PresenceHome::Home,
            HomeState::Checking => PresenceHome::Checking,
            HomeState::Away => PresenceHome::Away,
        }
    }

    /// When a running GPS check gives up: schedule one wake-up then and call [`Self::presence_tick`].
    pub fn presence_due_ms(&self) -> Option<i64> {
        self.watch().due_ms()
    }

    /// Whether the phone reports steps (a step counter it may read): the home circle needs them to see a walk out.
    pub fn presence_set_walk_sensor(&self, on: bool) {
        self.watch().set_walk_sensor(on);
    }

    /// The home circle in use, to draw on the map; `None` when home Wi-Fi alone decides.
    pub fn presence_circle(&self) -> Option<CircleOut> {
        self.watch().circle().map(|c| CircleOut { center: gp(c), radius_m: apgo_core::home_watch::HOME_CIRCLE_M })
    }

    /// When the walk that left home began (its first step), while away after a check.
    pub fn presence_left_at_ms(&self) -> Option<i64> {
        self.watch().left_at_ms()
    }

    // ---------- game setup ----------
    /// Archipelago player YAML for `player` from the solo options.
    ///
    /// # Errors
    /// Returns an error if a travel mode or the goal requirement is unknown, or the goals cannot be played.
    pub fn build_yaml(&self, player: String, o: SoloOptionsIn) -> Result<String, CoreError> {
        build_yaml(&player, &to_core(o)?).map_err(|detail| CoreError::Failed { detail })
    }

    /// Create and open a solo game.
    ///
    /// # Errors
    /// Returns an error if the options are invalid, a realm is missing or not scanned, or the game cannot be saved.
    #[allow(clippy::too_many_arguments)] // pre-existing: flat argument lists keep the exported/geometry call sites explicit
    pub fn start_solo(
        &self,
        game_id: String,
        name: String,
        o: SoloOptionsIn,
        zone_realms: Vec<String>,
        seed: u64,
        surface: String,
        avoid_stairs: bool,
        allow_parking: bool,
    ) -> Result<(), CoreError> {
        let opts = to_core(o)?;
        if zone_realms.len() != opts.zone_count as usize {
            return Err(err("pick one realm per zone"));
        }
        let generated = generate(&opts, seed).map_err(err)?;
        let realms = self.realm_atlases(&zone_realms)?;
        let home = self.home_for(&realms);
        let game = Game::create(
            NewGame {
                id: game_id,
                name,
                backend: Backend::Solo,
                seed_name: format!("solo-{seed}"),
                slot: generated.slot,
                zone_realms,
                realms: &realms,
                home,
                seed,
                solo_rewards: generated.rewards,
                surface: SurfacePref::parse(&surface),
                avoid_stairs,
                allow_parking,
            },
            &self.catalog,
        )
        .map_err(err)?;
        game.save(&self.dir).map_err(err)?;
        self.install(game);
        Ok(())
    }

    /// Create and open a game for an Archipelago multiworld from its `slot_data`.
    ///
    /// # Errors
    /// Returns an error if the `slot_data` is invalid, a realm is missing or not scanned, or the game cannot be saved.
    /// `address` and `slot_name` are where it connected: saved, so the app reconnects after a restart ([`Self::ap_saved_server`]).
    #[allow(clippy::too_many_arguments)] // pre-existing: flat argument lists keep the exported/geometry call sites explicit
    pub fn start_archipelago(
        &self,
        game_id: String,
        name: String,
        slot_json: String,
        seed_name: String,
        zone_realms: Vec<String>,
        seed: u64,
        surface: String,
        avoid_stairs: bool,
        allow_parking: bool,
        address: String,
        slot_name: String,
    ) -> Result<(), CoreError> {
        let slot = SlotData::from_json(&slot_json).map_err(err)?;
        let realms = self.realm_atlases(&zone_realms)?;
        let home = self.home_for(&realms);
        let mut game = Game::create(
            NewGame {
                id: game_id,
                name,
                backend: Backend::Archipelago,
                seed_name,
                slot,
                zone_realms,
                realms: &realms,
                home,
                seed,
                solo_rewards: BTreeMap::default(),
                surface: SurfacePref::parse(&surface),
                avoid_stairs,
                allow_parking,
            },
            &self.catalog,
        )
        .map_err(err)?;
        game.ap_server = Some(ApServer { address: address.trim().into(), slot: slot_name });
        game.save(&self.dir).map_err(err)?;
        self.install(game);
        Ok(())
    }

    /// How many zones a connected Archipelago game has, so the app can ask for one realm each.
    ///
    /// # Errors
    /// Returns an error if the `slot_data` is invalid.
    pub fn slot_zone_count(&self, slot_json: String) -> Result<u32, CoreError> {
        Ok(count_u32(SlotData::from_json(&slot_json).map_err(err)?.zones.len()))
    }

    /// Every saved game.
    pub fn games(&self) -> Vec<GameInfo> {
        Game::list_ids(&self.dir).into_iter().map(|(id, name)| GameInfo { id, name }).collect()
    }

    /// Open a saved game.
    ///
    /// # Errors
    /// Returns an error if the game file cannot be read or is corrupt.
    pub fn open_game(&self, id: String) -> Result<(), CoreError> {
        let g = Game::load(&self.dir, &id).map_err(err)?;
        self.install(g);
        Ok(())
    }

    /// Close the open game, saving it first. This is pausing: the next start no longer resumes it.
    pub fn close_game(&self) {
        Game::clear_playing(&self.dir);
        let mut game = self.game.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(g) = game.as_ref() {
            *self.last_game.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = Some(g.id.clone());
            if let Err(e) = g.save(&self.dir) {
                self.note(format!("could not save game {} on close: {e}", g.id));
            }
        }
        *game = None;
        self.set_zones(game, ZoneCache::default());
    }

    /// Delete a saved game; its file is archived, not erased.
    ///
    /// # Errors
    /// Never fails today; archiving problems are logged instead.
    pub fn delete_game(&self, id: String) -> Result<(), CoreError> {
        // The save moves to games-archive/ and the journal rows stay: a played game's data is evidence for diagnosing the next test.
        if let Err(e) = Game::archive(&self.dir, &id) {
            self.note(format!("could not archive game {id}: {e}"));
        }
        let was_open = {
            let mut g = self.game.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            let hit = g.as_ref().is_some_and(|x| x.id == id);
            if hit {
                *g = None;
                self.set_zones(g, ZoneCache::default());
            }
            hit
        };
        if was_open {
            Game::clear_playing(&self.dir);
        }
        Ok(())
    }

    /// The id of the open game, from memory (no disk access); `None` with no game open.
    pub fn open_game_id(&self) -> Option<String> {
        self.game_id()
    }

    /// Whether a game is open.
    pub fn has_game(&self) -> bool {
        self.game.lock().unwrap_or_else(std::sync::PoisonError::into_inner).is_some()
    }

    /// The game that was being played (opened and not paused) when the app last stopped: open it on start. `None` when the
    /// player paused, or that game was deleted.
    pub fn playing_game(&self) -> Option<String> {
        Game::playing(&self.dir)
    }

    // ---------- play ----------
    /// Every quest of the open game.
    pub fn quests(&self, now_ms: i64) -> Vec<QuestOut> {
        let units = self.unit_system();
        self.with_game(|g| {
            let by_id: HashMap<i64, &Assignment> = g.assignments.iter().map(|a| (a.location_id, a)).collect();
            g.quest_views(now_ms)
                .into_iter()
                .map(|q| {
                    // Worded from the view's target, which carries trap adjustments (a Slow trap doubles dwells).
                    let card = by_id.get(&q.location_id).map(|a| card::card_text(&Assignment { target: q.target.clone(), ..(*a).clone() }, g.home, units));
                    let (headline, card_detail) = card.map(|c| (c.headline, c.detail)).unwrap_or_default();
                    let progress_block = card::card_progress(&q, now_ms, units).map(|p| ProgressOut {
                        label: p.label,
                        value: p.value,
                        frac: p.frac,
                        hint: p.hint,
                        steps: p.steps,
                        step_at: p.step_at,
                    });
                    let place_row = card::place_row(&q.kind_id, &q.place, q.place_type.as_deref(), q.feature_id.is_some(), &self.catalog).map(|r| PlaceOut {
                        name: r.name,
                        kind: r.kind,
                        quoted: r.quoted,
                    });
                    let has_about = q.has_about;
                    let (shape, anchor, anchor_b, radius_m, path, detail) = describe(&q.target, units);
                    let collect = collect_out(&q.target, q.collected.as_ref(), &q.live_items, q.state);
                    let anchor = pin_at(collect.as_ref(), q.state, anchor, g.home);
                    QuestOut {
                        location_id: q.location_id,
                        zone: q.zone,
                        name: q.name,
                        place: q.place,
                        family: q.family,
                        kind_id: q.kind_id,
                        difficulty: q.difficulty,
                        tier: q.tier,
                        effort_min: q.effort_min,
                        mode: q.mode.name().into(),
                        badges: q.badges.into_iter().map(badge_out).collect(),
                        pin_badge: q.pin_badge.map(badge_out),
                        badge_note: q.badge_note,
                        state: match q.state {
                            QuestState::Locked => "locked",
                            QuestState::Hidden => "hidden",
                            QuestState::Open => "open",
                            QuestState::InProgress => "progress",
                            QuestState::Done => "done",
                        }
                        .into(),
                        progress: q.progress,
                        shape: shape.into(),
                        anchor: anchor.map(gp),
                        anchor_b: anchor_b.map(gp),
                        radius_m,
                        path: path.into_iter().map(gp).collect(),
                        detail,
                        fallback: q.fallback,
                        boss: q.boss,
                        blurb: q.blurb,
                        reward: q.reward,
                        chain_id: q.chain_id,
                        collect,
                        headline,
                        card_detail,
                        place_row,
                        progress_block,
                        has_about,
                    }
                })
                .collect()
        })
        .unwrap_or_default()
    }

    /// The shared backpack of the open game; `None` with no game open.
    pub fn bag(&self) -> Option<BagOut> {
        self.with_game(|g| {
            let items = g
                .assignments
                .iter()
                .filter(|a| !g.done.contains(&a.location_id))
                .filter_map(|a| match &a.target {
                    Target::Collect { theme, .. } => g.collected.get(&a.location_id).map(|c| (theme, c.carried)),
                    _ => None,
                })
                .flat_map(|(theme, n)| std::iter::repeat_n(theme.clone(), usize::try_from(n).unwrap_or(0)))
                .collect();
            let carried = g.carried_total();
            #[allow(clippy::cast_possible_truncation)] // a factor between 0.5 and 1
            let step_factor = g.step_factor() as f32;
            BagOut { size: g.bag_size, items, over: carried.saturating_sub(g.bag_size), step_factor, load: apgo_core::forage::load_text(carried, g.bag_size) }
        })
    }

    /// The about sheet of quest `location_id`'s place. Reads the realm's saved scan, so call it off the UI thread. `None` with no
    /// game open, for a quest with no real place, when the map knows nothing more, or when the place is gone from the realm's
    /// current scan (rescanned).
    pub fn about(&self, location_id: i64) -> Option<AboutOut> {
        let (realm_id, feature_id) = self.with_game(|g| g.quest_feature(location_id).map(|(r, f)| (r.to_owned(), f.to_owned())))??;
        let atlas = self.store().load_atlas(&realm_id)?; // read with the game lock released
        let f = atlas.features.iter().find(|f| f.id == feature_id)?;
        let a = apgo_core::poi_details::about(f, self.unit_system())?;
        Some(AboutOut {
            description: a.description,
            rows: a.rows.into_iter().map(|(label, value)| AboutRowOut { label, value }).collect(),
            wikipedia_url: a.wikipedia_url,
        })
    }

    /// The park quest (shown on the map, not hidden) whose outline holds the point, for a tap anywhere inside a park; the innermost
    /// when parks nest.
    pub fn park_at(&self, lat: f64, lon: f64, now_ms: i64) -> Option<i64> {
        self.with_game(|g| {
            let views = g.quest_views(now_ms);
            let parks = views.iter().filter(|q| q.state != QuestState::Hidden).filter_map(|q| match &q.target {
                Target::DwellArea { poly, .. } => Some((q.location_id, poly.as_slice())),
                _ => None,
            });
            apgo_core::geo::smallest_containing(Point::new(lat, lon), parks)
        })
        .flatten()
    }

    /// The zones of the open game.
    pub fn zones(&self) -> Vec<ZoneOut> {
        // Lock order: game, then zones.
        self.with_game(|g| {
            let names = &self.zones.lock().unwrap_or_else(std::sync::PoisonError::into_inner).names;
            g.slot
                .zones
                .iter()
                .enumerate()
                .map(|(i, z)| ZoneOut {
                    id: z.id,
                    unlocked: g.zone_unlocked(z.id),
                    keys_needed: z.zone_keys_needed,
                    realm_name: names.get(i).cloned().unwrap_or_default(),
                    realm_id: g.zone_realms.get(i).cloned().unwrap_or_default(),
                })
                .collect()
        })
        .unwrap_or_default()
    }

    /// What the Play screen shows, at `now_ms`; `None` with no game open.
    pub fn hud(&self, now_ms: i64) -> Option<HudOut> {
        self.with_game(|g| {
            let s = g.goal_status();
            let views = g.quest_views(now_ms);
            let mut letters: Vec<char> = g.items.iter().filter_map(|i| i.strip_prefix("Letter ").and_then(|s| s.chars().next())).collect();
            letters.sort_unstable();
            let tools: Vec<String> =
                Mode::ALL.iter().filter_map(|m| apgo_core::solo::badge_for(*m)).filter(|t| g.items.iter().any(|i| i == t)).map(ToString::to_string).collect();
            HudOut {
                away_running: g.away_running(),
                goals: g
                    .goal_statuses()
                    .into_iter()
                    .map(|(id, st)| GoalLineOut { id, label: st.label, progress: st.progress, achieved: st.achieved })
                    .collect(),
                goal_label: s.label,
                goal_progress: s.progress,
                goal_achieved: s.achieved,
                done: count_u32(views.iter().filter(|v| v.state == QuestState::Done).count()),
                total: count_u32(views.len()),
                keys: count_u32(g.items.iter().filter(|i| *i == "Progressive Zone Key").count()),
                tools,
                letters: letters.into_iter().collect(),
                danger: danger_line(&g.trap_labels(), g.blocked_reason()),
                traps: g.trap_labels(),
                thaw: g.traps.thaw_point().map(gp),
                waypoint: g.traps.waypoint().map(gp),
                blocked: g.blocked_reason(),
                distance_km: g.stats.distance_m / 1000.0,
                streak_days: g.streak_days(now_ms),
                fog: g.slot.fog_of_war,
                backend: if g.backend == Backend::Solo { "solo".into() } else { "archipelago".into() },
                game_name: g.name.clone(),
            }
        })
    }

    /// Feed a position fix (and the step counter, if any) to the open game; returns what happened.
    pub fn on_fix(&self, fix: FixIn, steps: Option<i64>, simulated: bool) -> Vec<EventOut> {
        let simulated = simulated && SIM_ALLOWED;
        let raw = raw_fix(&fix, simulated, SIM_ALLOWED);
        let t_ms = raw.t_ms;
        if !simulated {
            let mut watch = self.watch();
            watch.on_fix(raw.point(), raw.accuracy_m, t_ms);
            // At home (or still telling, inside the circle) a fix reaches nothing: no points or trace at home.
            if watch.has_center() && watch.state() != HomeState::Away {
                return Vec::new();
            }
        }
        let Some((game_id, ev, entries, near, est)) = self.with_game(|g| {
            // One pass over the zone shapes per fix, at this fix's own estimate: the proximity presence reads next.
            let ev = g.on_fix_with_zone(&raw, steps, |p| self.set_proximity(self.zone_distance_m(p)));
            // No estimate yet: the fix was unusable and places nobody (presence still learns roughly where it was).
            let est = g.last_estimate();
            if est.is_none() {
                self.set_proximity(self.zone_distance_m(raw.point()));
            }
            self.save_if_due(g, t_ms, ev.iter().any(Event::saves_at_once));
            let at = est.map(|e| (e.lat, e.lon));
            let entries = g.journal_events(&ev, t_ms, at);
            let near = est.map(|e| g.explain_near(&e, NEAR_MISS_RADIUS_M)).unwrap_or_default();
            (g.id.clone(), ev, entries, near, est)
        }) else {
            return Vec::new();
        };
        let at = est.map(|e| (e.lat, e.lon));
        let point = est
            .filter(|e| e.accepted)
            .map(|e| TrackPoint { t_ms, lat: e.lat, lon: e.lon, accuracy_m: e.uncertainty_m, simulated })
            .filter(|p| self.journal_point_due(p));
        let rejected = est.map_or_else(|| Some("unusable fix".into()), |e| rejected_detail(&e, self.unit_system())).filter(|_| self.reject_log_due(t_ms));
        // One transaction per fix: the point and its log lines cost one commit.
        self.journal_do(|j| {
            j.batch(|j| {
                if let Some(p) = &point {
                    j.add_point(&game_id, p)?;
                }
                if let Some(detail) = rejected {
                    j.log(&game_id, &JournalEvent { t_ms, kind: kind::FIX_REJECTED.into(), detail, at })?;
                }
                for n in self.new_near_misses(near, t_ms) {
                    j.log(
                        &game_id,
                        &JournalEvent {
                            t_ms,
                            kind: kind::NEAR_MISS.into(),
                            detail: format!("{}: {} ({})", n.name, n.reason, distance(n.distance_m, self.unit_system())),
                            at,
                        },
                    )?;
                }
                entries.iter().try_for_each(|e| j.log(&game_id, e))
            })
        });
        self.events_out(ev)
    }

    /// The fix spacing the phone now asks for: interval plus batching delay, ms (`None`: unknown). Call it on every location
    /// (re)start; the gap bridge waits it out, so sparse screen-off fixes keep counting.
    pub fn set_fix_spacing_hint(&self, ms: Option<i64>) {
        *self.fix_spacing_hint.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = ms;
        self.with_game(|g| g.set_fix_spacing_hint(ms)); // now: steps may bridge before the next fix
    }

    /// A compass reading from the phone (at most 2 Hz).
    pub fn on_heading(&self, h: HeadingIn) {
        let core = heading_in(&h);
        self.with_game(|g| g.on_heading(&core));
    }

    /// The open game's last accepted position, kept across counting pauses (traps are placed there, re-review N5); `None` with no game
    /// or before the first one. For logic, where [`Self::position`] is for drawing (adversarial review M3).
    pub fn last_accepted_pos(&self) -> Option<GeoPoint> {
        self.with_game(|g| g.last_accepted_pos()).flatten().map(gp)
    }

    /// What the map shows for the player at `now_ms`; `None` with no game or before the first fix.
    pub fn position(&self, now_ms: i64) -> Option<PositionOut> {
        self.with_game(|g| g.position(now_ms)).flatten().map(|d| position_out(&d))
    }

    /// The progressive quests of the open game, one entry per bar.
    pub fn chains(&self, now_ms: i64) -> Vec<ChainOut> {
        self.with_game(|g| {
            g.chain_views(now_ms)
                .into_iter()
                .map(|c| ChainOut {
                    id: c.id,
                    zone: c.zone,
                    kind_id: c.kind_id,
                    name: c.name,
                    family: c.family,
                    unit: match c.unit {
                        ChainUnit::Steps => "steps",
                        ChainUnit::Minutes => "minutes",
                        ChainUnit::Cells => "cells",
                    }
                    .into(),
                    counter: c.counter,
                    total: c.total,
                    rule: c.rule,
                    marks: c.marks.into_iter().map(|m| MarkOut { at: m.at, location_id: m.location_id, reached: m.reached, reward: m.reward }).collect(),
                    badges: c.badges.into_iter().map(badge_out).collect(),
                    badge_note: c.badge_note,
                })
                .collect()
        })
        .unwrap_or_default()
    }

    /// Load the saved step calibration into the open game's filter.
    pub fn set_step_calibration(&self, c: StepCalIn) {
        self.with_game(|g| g.set_step_calibration(step_cal_in(c)));
    }

    /// The open game's step calibration, to save; `None` with no game or before anything was learned.
    pub fn step_calibration(&self) -> Option<StepCalOut> {
        self.with_game(|g| g.step_calibration()).and_then(step_cal_to_save)
    }

    /// A step-counter reading from the phone (cumulative since boot) at the sensor event's time, with the cadence if the phone knows it.
    /// Only counts while a game is open.
    pub fn on_steps(&self, total: i64, t_ms: i64, cadence: Option<f64>) -> Vec<EventOut> {
        self.watch().on_steps(total, t_ms);
        let Some((game_id, ev, entries)) = self.with_game(|g| {
            let ev = g.on_steps(total, t_ms, cadence);
            self.save_if_due(g, t_ms, ev.iter().any(Event::saves_at_once));
            let entries = g.journal_events(&ev, t_ms, None);
            (g.id.clone(), ev, entries)
        }) else {
            return Vec::new();
        };
        self.journal_do(|j| entries.iter().try_for_each(|e| j.log(&game_id, e)));
        self.events_out(ev)
    }

    /// The phone arrived home (home Wi-Fi or the home circle): forager quests bank what they carry and the walk closes into its
    /// recap (see `Game::arrive_home`), kept with the game until dismissed. Safe to call again.
    pub fn bank_at_home(&self, t_ms: i64) -> Vec<EventOut> {
        // Read the trace first (no game lock held), so closing the outing, storing the recap and saving happen in one pass.
        let Some((game_id, start)) = self.with_game(|g| (g.id.clone(), g.outing.as_ref().map(|o| o.start_ms))) else {
            return Vec::new();
        };
        let segs = start.map_or_else(Vec::new, |from| self.read_segments(&game_id, from, t_ms));
        let Some((id, ev, entries)) = self.with_game(|g| {
            let (ev, recap) = g.arrive_home(t_ms);
            let stored = recap.is_some_and(|mut r| {
                if g.id == game_id {
                    r.set_route(&segs.iter().map(|s| s.iter().map(|p| Point::new(p.lat, p.lon)).collect()).collect::<Vec<_>>());
                }
                g.recap = Some(r);
                true
            });
            if stored {
                if let Err(e) = g.save(&self.dir) {
                    self.note(format!("could not save game {}: {e}", g.id));
                }
            } else {
                self.save_if_due(g, t_ms, true);
            }
            let entries = g.journal_events(&ev, t_ms, None);
            (g.id.clone(), ev, entries)
        }) else {
            return Vec::new();
        };
        self.journal_do(|j| entries.iter().try_for_each(|e| j.log(&id, e)));
        self.events_out(ev)
    }

    /// The last walk's recap, until dismissed.
    pub fn pending_recap(&self) -> Option<RecapOut> {
        self.with_game(|g| g.recap.as_ref().map(RecapOut::from)).flatten()
    }

    /// The player closed the recap.
    pub fn dismiss_recap(&self) {
        self.with_game(|g| {
            if g.recap.take().is_some() {
                if let Err(e) = g.save(&self.dir) {
                    self.note(format!("could not save game {}: {e}", g.id));
                }
            }
        });
    }

    /// Archipelago: pass the full received-item name list; new items trigger unlocks/traps.
    pub fn sync_items(&self, items: Vec<String>, now_ms: i64, pos: Option<GeoPoint>) -> Vec<EventOut> {
        let dir = self.dir.clone();
        let at = pos.as_ref().map(|p| (p.lat, p.lon));
        let Some((game_id, ev, entries)) = self.with_game(|g| {
            let ev = g.sync_items(&items, now_ms, pos.as_ref().map(pt));
            if !ev.is_empty() {
                let _ = g.save(&dir);
            }
            let entries = g.journal_events(&ev, now_ms, at);
            (g.id.clone(), ev, entries)
        }) else {
            return Vec::new();
        };
        self.journal_do(|j| entries.iter().try_for_each(|e| j.log(&game_id, e)));
        self.events_out(ev)
    }

    /// Record locations the server already has as checked.
    pub fn mark_checked(&self, ids: Vec<i64>, now_ms: i64) {
        let dir = self.dir.clone();
        self.with_game(|g| {
            g.mark_checked(&ids, now_ms);
            let _ = g.save(&dir);
        });
    }

    /// Whether the open game belongs to the connected Archipelago server, `identity` being its `seed/slot`
    /// (`ApSession::server_identity`, stored at the game's start): its checks, goal and items are only exchanged with that one.
    /// A game started before games remembered their server (seed name `archipelago`), or no game open, is `Unknown`.
    pub fn ap_server_match(&self, identity: String) -> ServerMatch {
        self.with_game(|g| match g.seed_name.as_str() {
            LEGACY_AP_SEED => ServerMatch::Unknown,
            own if own == identity => ServerMatch::Same,
            _ => ServerMatch::Other,
        })
        .unwrap_or(ServerMatch::Unknown)
    }

    /// Where the open Archipelago game connects, saved at its start: the app connects there on its own when the game opens (after
    /// an app restart too). `None` for a solo game, a game from before servers were saved, or no game open.
    pub fn ap_saved_server(&self) -> Option<ApServerOut> {
        self.with_game(|g| g.ap_server.clone().filter(|_| g.backend == Backend::Archipelago))
            .flatten()
            .map(|s| ApServerOut { address: s.address, slot: s.slot })
    }

    /// Where the app should connect now that a game was opened or started, given the session it has (`connected`, `None` for
    /// none): the open game's saved server when the session is elsewhere or missing (after an app restart, or another game's
    /// server), else `None` (keep the session). Called on game open, not on every refresh, so a player connecting elsewhere from
    /// New Game is not pulled back.
    pub fn ap_server_to_connect(&self, connected: Option<ApServerOut>) -> Option<ApServerOut> {
        self.ap_saved_server().filter(|saved| connected.as_ref() != Some(saved))
    }

    /// Point the open Archipelago game at a new address (an archipelago.gg room that moved to another port). Its seed and slot stay,
    /// so it is still the same game on the server ([`Self::ap_server_match`]).
    ///
    /// # Errors
    /// `Failed` when `address` is blank, the open game has no saved server, or it cannot be saved.
    pub fn set_ap_address(&self, address: String) -> Result<(), CoreError> {
        let address = address.trim();
        if address.is_empty() {
            return Err(CoreError::Failed { detail: "the server address is empty".into() });
        }
        let dir = self.dir.clone();
        self.with_game(|g| {
            let server = g.ap_server.as_mut().filter(|_| g.backend == Backend::Archipelago).ok_or("this game has no saved server")?;
            server.address = address.into();
            g.save(&dir)
        })
        .unwrap_or_else(|| Err("no game open".into()))
        .map_err(err)
    }

    /// What the open game tells the Archipelago server `identity` on every (re)connect: every finished location and whether the goal
    /// was reached. The protocol asks clients to send checks made while offline on connection, and the server ignores repeats, so
    /// this is all of them, every time. `None` unless the game belongs to that server ([`Self::ap_server_match`]): another seed's
    /// server must never get them (a goal there could release its whole world).
    pub fn server_report(&self, identity: String) -> Option<ServerReportOut> {
        (self.ap_server_match(identity) == ServerMatch::Same)
            .then(|| self.with_game(|g| ServerReportOut { checks: g.done.iter().copied().collect(), goal: g.goal_reported }))
            .flatten()
    }

    /// Messages the core queued for the diagnostics log since the last call (journal failures and the like).
    pub fn take_diag(&self) -> Vec<String> {
        std::mem::take(&mut *self.diag.lock().unwrap_or_else(std::sync::PoisonError::into_inner))
    }

    // ---------- track and audit ----------
    /// Record that the app went to the foreground or background (the audit trail needs to know when tracking could not run).
    pub fn log_app_state(&self, foreground: bool, t_ms: i64) {
        let Some(id) = self.game_id() else { return };
        let k = if foreground { kind::APP_FOREGROUND } else { kind::APP_BACKGROUND };
        self.journal_do(|j| j.log(&id, &JournalEvent { t_ms, kind: k.into(), detail: String::new(), at: None }));
    }

    /// The newest `limit` things that happened in the open game, newest first: quests with how they were done, rewards with
    /// where they came from, traps, near misses and notices.
    pub fn activity(&self, limit: u32) -> Vec<AuditEventOut> {
        let id = self.game_id().or_else(|| self.last_game.lock().unwrap_or_else(std::sync::PoisonError::into_inner).clone());
        let (Some(id), Some(j)) = (id, self.journal.as_ref()) else { return Vec::new() };
        let rows = j.lock().unwrap_or_else(std::sync::PoisonError::into_inner).recent_events(&id, limit).unwrap_or_default();
        rows.into_iter().map(|e| AuditEventOut { t_ms: e.t_ms, kind: e.kind, detail: e.detail, at: e.at.map(|(lat, lon)| GeoPoint { lat, lon }) }).collect()
    }

    /// Record that the player paused play (tracking turns off) or resumed it. Call before `close_game` when pausing.
    pub fn log_session(&self, resumed: bool, t_ms: i64) {
        let Some(id) = self.game_id() else { return };
        let k = if resumed { kind::PLAY_RESUMED } else { kind::PLAY_PAUSED };
        self.journal_do(|j| j.log(&id, &JournalEvent { t_ms, kind: k.into(), detail: String::new(), at: None }));
    }

    /// "inside" | "near" | "far" for where the last fix was relative to the open game's zones; "unknown" with no fix or no zones.
    pub fn last_zone_proximity(&self) -> String {
        let best = *self.last_proximity.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        match best {
            Some(Proximity::Inside) => "inside",
            Some(Proximity::Near) => "near",
            Some(Proximity::Far) => "far",
            None => "unknown",
        }
        .into()
    }

    /// Presence rules (home Wi-Fi, car) turn counting off and on.
    pub fn set_counting(&self, on: bool, t_ms: i64) {
        // Leaving by the home circle: credit steps from where the walk out began, not from pacing indoors (the counter runs at home).
        let left = self.watch().left_steps().filter(|_| on);
        self.with_game(|g| {
            if let Some((total, at)) = left {
                g.set_leave_mark(total, at);
            }
            g.set_counting(on, t_ms);
        });
    }

    /// When the next time-away mark falls due if nothing changes: the app schedules one wake-up then and calls [`Self::tick`].
    pub fn next_due_ms(&self, now_ms: i64) -> Option<i64> {
        self.with_game(|g| g.next_due_ms(now_ms)).flatten()
    }

    /// A scheduled wake-up: credit time away up to `t_ms` and report what completed.
    pub fn tick(&self, t_ms: i64) -> Vec<EventOut> {
        let Some((id, entries, ev)) = self.with_game(|g| {
            let ev = g.tick(t_ms);
            self.save_if_due(g, t_ms, ev.iter().any(Event::saves_at_once));
            (g.id.clone(), g.journal_events(&ev, t_ms, None), ev)
        }) else {
            return Vec::new();
        };
        self.journal_do(|j| entries.iter().try_for_each(|e| j.log(&id, e)));
        self.events_out(ev)
    }

    /// Record a presence change ("Home Wi-Fi connected, paused") in the activity log.
    pub fn log_presence(&self, text: String, t_ms: i64) {
        let Some(id) = self.game_id() else { return };
        self.journal_do(|j| j.log(&id, &JournalEvent { t_ms, kind: kind::PRESENCE.into(), detail: text, at: None }));
    }

    /// When the app was last sent to the background in the open game: the start of "while you were out".
    pub fn last_background_ms(&self) -> Option<i64> {
        let id = self.game_id()?;
        let j = self.journal.as_ref()?.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        j.last_of_kind(&id, kind::APP_BACKGROUND).ok().flatten()
    }

    /// A number that changes whenever something is written to the activity log: reload it only when this moves.
    pub fn journal_revision(&self) -> u64 {
        self.journal.as_ref().map_or(0, |j| j.lock().unwrap_or_else(std::sync::PoisonError::into_inner).revision())
    }

    /// The trace of the open game as separate lines, simplified for drawing: standing still collapses to one spot and GPS wobble is
    /// smoothed out (the saved points are untouched).
    pub fn track(&self, from_ms: i64, to_ms: i64) -> Vec<TrackSegmentOut> {
        let (Some(id), Some(j)) = (self.game_id(), self.journal.as_ref()) else { return Vec::new() };
        let segs = j.lock().unwrap_or_else(std::sync::PoisonError::into_inner).segments(&id, from_ms, to_ms, DEFAULT_MAX_GAP_MS).unwrap_or_default();
        segs.into_iter()
            .map(|s| {
                let pts: Vec<Point> = s.iter().map(|p| Point::new(p.lat, p.lon)).collect();
                TrackSegmentOut {
                    points: simplify_spaced(&pts, TRACE_MIN_STEP_M, TRACE_TOLERANCE_M).into_iter().map(|p| GeoPoint { lat: p.lat, lon: p.lon }).collect(),
                }
            })
            .collect()
    }

    /// The current session's trace, matched to streets where confident: a full load. The app draws from
    /// [`Self::trace_matched_since`] deltas; this stays for tools and tests that want the whole line at once.
    pub fn trace_matched(&self) -> TraceOut {
        let (from_ms, runs) = self.with_game(|g| g.trace_matched()).unwrap_or((None, vec![]));
        TraceOut { from_ms, runs: runs.into_iter().map(line).collect() }
    }

    /// What changed in [`Self::trace_matched`] since `cursor` (0 for a full load): new settled points to append and the whole
    /// provisional tail; with `reset`, everything, to replace what the map holds (another game, a line thinned at its memory cap, an unknown cursor).
    pub fn trace_matched_since(&self, cursor: u64) -> TraceDelta {
        match self.with_game(|g| g.trace_matched_since(cursor)) {
            Some(d) => TraceDelta {
                reset: d.reset,
                cursor: d.cursor,
                from_ms: d.from_ms,
                joins: d.joins,
                append: d.append.into_iter().map(line).collect(),
                tail: line(d.tail),
            },
            None => TraceDelta { reset: true, cursor: 0, from_ms: None, joins: false, append: vec![], tail: line(vec![]) },
        }
    }

    /// Reroll unfinished quests (all if `ids` is empty). Returns how many were re-placed.
    ///
    /// # Errors
    /// Returns an error if no game is open, or a zone has no realm.
    pub fn reroll(&self, ids: Vec<i64>, seed: u64) -> Result<u32, CoreError> {
        let (realm_ids, all): (Vec<String>, Vec<i64>) =
            self.with_game(|g| (g.zone_realms.clone(), g.assignments.iter().map(|a| a.location_id).collect())).ok_or_else(|| err("no game open"))?;
        let realms = self.realm_atlases(&realm_ids)?;
        let ids = if ids.is_empty() { all } else { ids };
        let dir = self.dir.clone();
        let catalog = &self.catalog;
        self.with_game(|g| {
            g.reroll(&ids, &realms, seed, catalog).map(|n| {
                let _ = g.save(&dir);
                count_u32(n)
            })
        })
        .ok_or_else(|| err("no game open"))?
        .map_err(err)
    }

    /// Write the open game to disk now (the app calls this when it goes to the background).
    pub fn save_game(&self) {
        self.with_game(|g| {
            if let Err(e) = g.save(&self.dir) {
                self.note(format!("could not save game {}: {e}", g.id));
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use apgo_core::geo::destination;
    use apgo_core::scan::{build_atlas, Feature};

    fn home() -> Point {
        Point::new(40.0, -111.0)
    }

    fn circle(center: Point, radius_m: f64) -> CircleOut {
        CircleOut { center: gp(center), radius_m }
    }

    /// An engine in its own new directory, removed again when the test ends (even when it fails).
    struct TestEngine {
        e: Arc<Engine>,
        dir: PathBuf,
    }

    impl std::ops::Deref for TestEngine {
        type Target = Engine;
        fn deref(&self) -> &Engine {
            &self.e
        }
    }

    impl Drop for TestEngine {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    /// An engine in a fresh directory with one scanned realm "r0" (a 2 km circle at `home`) and a walking game on it open.
    fn engine_with_game(name: &str) -> TestEngine {
        // Unique per run as well as per test: a directory left by an earlier run (same pid) is never reused.
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos());
        let dir = std::env::temp_dir().join(format!("apgo-engine-{name}-{}-{nanos}", std::process::id()));
        let e = TestEngine { e: Engine::new(dir.to_string_lossy().into_owned()), dir };
        e.save_realm("r0".into(), "R0".into(), None, Some(circle(home(), 2000.0)), vec![], false).unwrap();
        let streets = (-10..=10).flat_map(|n| (-10..=10).map(move |k| destination(destination(home(), 0.0, f64::from(n) * 150.0), 90.0, f64::from(k) * 150.0)));
        e.store().save_atlas(&build_atlas("r0", 0, vec![], streets.collect(), &e.catalog)).unwrap();
        let opts = SoloOptions {
            zone_count: 1,
            run_quests: false,
            run_share: 20,
            number_of_trips: 5,
            goal: "all_trips".into(),
            quest_types: vec!["reach".into()],
            ..SoloOptions::default()
        };
        let generated = generate(&opts, 1).unwrap();
        let realms = e.realm_atlases(&["r0".into()]).unwrap();
        let game = Game::create(
            NewGame {
                id: "g1".into(),
                name: "Test".into(),
                backend: Backend::Solo,
                seed_name: "s".into(),
                slot: generated.slot,
                zone_realms: vec!["r0".into()],
                realms: &realms,
                home: home(),
                seed: 1,
                solo_rewards: generated.rewards,
                surface: SurfacePref::Any,
                avoid_stairs: false,
                allow_parking: true,
            },
            &e.catalog,
        )
        .unwrap();
        e.install(game);
        e
    }

    #[test]
    fn arriving_home_stores_one_recap_with_its_route_until_dismissed() {
        let e = engine_with_game("recap");
        e.set_counting(false, 0);
        e.set_counting(true, 1_000);
        e.with_game(|g| g.outing.as_mut().unwrap().steps = 900);
        let east = |m: f64| destination(home(), 90.0, m);
        e.journal_do(|j| {
            (0..=10).try_for_each(|i| {
                let p = east(500.0 + f64::from(i) * 10.0);
                j.add_point("g1", &TrackPoint { t_ms: 60_000 + i64::from(i) * 5_000, lat: p.lat, lon: p.lon, accuracy_m: 5.0, simulated: false })
            })
        });
        e.set_counting(false, 599_000);
        e.bank_at_home(600_000);
        let r = e.pending_recap().expect("a 10 min walk with 900 steps");
        assert_eq!((r.steps, r.route.len(), r.route[0].len()), (900, 1, 2), "a straight line thins to its ends");
        assert!((r.distance_m - 100.0).abs() < 1.0);
        e.bank_at_home(610_000);
        assert!(e.pending_recap().is_some_and(|r| r.end_ms == 600_000), "a second arrival keeps the recap");
        let restarted = Engine::new(e.dir.to_string_lossy().into_owned());
        restarted.open_game("g1".into()).unwrap();
        assert!(restarted.pending_recap().is_some(), "saved with the game");
        restarted.dismiss_recap();
        assert!(restarted.pending_recap().is_none());
    }

    #[test]
    fn a_walk_with_no_trace_still_gets_its_recap() {
        let e = engine_with_game("recap-no-trace");
        e.set_counting(false, 0);
        e.set_counting(true, 1_000);
        e.with_game(|g| g.outing.as_mut().unwrap().steps = 900);
        e.bank_at_home(600_000);
        assert!(e.pending_recap().is_some_and(|r| r.distance_m == 0.0 && r.route.is_empty()));
    }

    #[test]
    fn opening_an_old_game_links_its_quests_to_their_places_and_saves_it() {
        let e = engine_with_game("relink");
        let bench =
            Feature { id: "n77".into(), point: home(), name: None, tags: BTreeMap::from([("amenity".to_string(), "bench".to_string())]), geometry: vec![] };
        let streets = (-10..=10).flat_map(|n| (-10..=10).map(move |k| destination(destination(home(), 0.0, f64::from(n) * 150.0), 90.0, f64::from(k) * 150.0)));
        e.store().save_atlas(&build_atlas("r0", 0, vec![bench], streets.collect(), &e.catalog)).unwrap();
        // A quest saved before quests kept their place: a bench dwell with no feature id; another quest keeps the id it has.
        let (old, kept) = e
            .with_game(|g| {
                let (old, kept) = (g.assignments[0].location_id, g.assignments[1].location_id);
                let a = &mut g.assignments[0];
                (a.kind_id, a.place, a.feature_id) = ("bench_warmer".into(), "Bench Warmer".into(), None);
                a.target = Target::Dwell { p: destination(home(), 0.0, 30.0), r: 40.0, minutes: 3.0 };
                g.assignments[1].feature_id = Some("w1".into());
                g.save(&e.dir).unwrap();
                (old, kept)
            })
            .unwrap();
        e.open_game("g1".into()).unwrap();
        let kind = |id: i64| e.quests(0).into_iter().find(|q| q.location_id == id).unwrap().place_row.and_then(|p| p.kind);
        assert_eq!(kind(old), None, "not linked before the streets build");
        e.refresh_streets(None).unwrap();
        assert_eq!(kind(old).as_deref(), Some("Bench"), "the card names what the place is once linked");
        let saved = Game::load(&e.dir, "g1").unwrap();
        let ids = |id: i64| saved.assignments.iter().find(|a| a.location_id == id).unwrap().feature_id.clone();
        assert_eq!((ids(old).as_deref(), ids(kept).as_deref()), (Some("n77"), Some("w1")), "linked once and saved; a linked quest is left alone");
    }

    /// Turns the first `n` quests of the test game into foragers (need 3, one placed item each) carrying `carried[i]` of `themes[i]`.
    fn with_foragers(e: &TestEngine, themes: &[&str], carried: &[u32]) -> Vec<i64> {
        e.with_game(|g| {
            themes
                .iter()
                .zip(carried)
                .enumerate()
                .map(|(i, (theme, n))| {
                    let a = &mut g.assignments[i];
                    a.kind_id = "forager".into();
                    a.target = Target::Collect {
                        pts: vec![destination(home(), 0.0, 400.0 + 100.0 * f64::from(u32::try_from(i).unwrap()))],
                        need: 3,
                        r: 25.0,
                        theme: (*theme).into(),
                    };
                    g.collected.insert(a.location_id, Collected { carried: *n, ..Default::default() });
                    a.location_id
                })
                .collect()
        })
        .unwrap()
    }

    #[test]
    fn the_bag_holds_what_every_forager_carries() {
        let e = engine_with_game("bag");
        assert!(e.bag().is_some_and(|b| b.items.is_empty() && b.load == "Empty" && b.size == 20));
        with_foragers(&e, &["acorns", "diamonds"], &[2, 1]);
        let b = e.bag().unwrap();
        assert_eq!((b.items, b.over, b.load.as_str()), (vec!["acorns".to_string(), "acorns".into(), "diamonds".into()], 0, "Light"));
        with_foragers(&e, &["acorns", "diamonds"], &[20, 2]);
        let b = e.bag().unwrap();
        assert_eq!((b.over, b.load.as_str()), (2, "Heavy · steps −10%"));
        assert!((b.step_factor - 0.9).abs() < 1e-6);
    }

    #[test]
    fn a_forager_shows_its_live_items_and_the_streets_build_fills_it() {
        let e = engine_with_game("forage");
        let id = with_foragers(&e, &["acorns"], &[0])[0];
        let items = |e: &TestEngine| e.quests(0).into_iter().find(|q| q.location_id == id).unwrap().collect.unwrap().items;
        assert_eq!(items(&e).len(), 1, "the placed item until a build gives it a pool");
        assert!(items(&e).iter().all(|i| !i.picked), "only items still out there are listed");
        e.refresh_streets(None).unwrap();
        assert_eq!(items(&e).len(), 9, "3 x need");
    }

    #[test]
    fn about_reads_the_quest_place_from_the_realm_scan_and_is_none_once_it_is_gone() {
        let e = engine_with_game("about");
        let id = e.quests(0)[0].location_id;
        let quest = |e: &TestEngine| e.quests(0).into_iter().find(|q| q.location_id == id).unwrap();
        assert!(!quest(&e).has_about, "a street quest has no place");
        assert!(e.about(id).is_none());
        let artwork = Feature {
            id: "n42".into(),
            point: home(),
            name: Some("Allow Me".into()),
            tags: BTreeMap::from([
                ("tourism".to_string(), "artwork".to_string()),
                ("artwork_type".to_string(), "statue".to_string()),
                ("artist_name".to_string(), "J. Seward Johnson".to_string()),
                ("wikipedia".to_string(), "en:Allow Me".to_string()),
            ]),
            geometry: vec![],
        };
        e.store().save_atlas(&build_atlas("r0", 0, vec![artwork], vec![], &e.catalog)).unwrap();
        e.with_game(|g| {
            let a = g.assignments.iter_mut().find(|a| a.location_id == id).unwrap();
            (a.feature_id, a.place_type, a.has_about) = (Some("n42".into()), Some("Artwork".into()), true);
        });
        assert!(quest(&e).has_about);
        let d = e.about(id).expect("the artwork's about sheet");
        let rows: Vec<(&str, &str)> = d.rows.iter().map(|r| (r.label.as_str(), r.value.as_str())).collect();
        assert_eq!(rows, [("Kind", "Statue"), ("Artist", "J. Seward Johnson")]);
        assert_eq!(d.wikipedia_url.as_deref(), Some("https://en.wikipedia.org/wiki/Allow_Me"));
        e.store().save_atlas(&build_atlas("r0", 0, vec![], vec![], &e.catalog)).unwrap();
        assert!(e.about(id).is_none(), "rescanned without the artwork");
        assert!(e.about(-1).is_none(), "no such quest");
    }

    #[test]
    fn quests_carry_the_card_headline_detail_and_progress() {
        let e = engine_with_game("card");
        let id = e
            .with_game(|g| {
                let a = &mut g.assignments[0];
                (a.kind_id, a.place, a.feature_id) = ("street_smarts".into(), "Street Smarts".into(), None);
                a.target = Target::Point { p: destination(home(), 0.0, 300.0), r: 40.0 };
                a.location_id
            })
            .unwrap();
        let q = e.quests(0).into_iter().find(|q| q.location_id == id).unwrap();
        assert_eq!(q.headline, "Get here");
        assert!(q.card_detail.starts_with("Within "), "{}", q.card_detail);
        assert!(q.progress_block.is_none(), "reaching a point has no progress block");
        assert!(q.place_row.is_none() && !q.has_about, "a street spot is no place");
        // Under a Slow trap the headline says the time the tracker actually needs.
        e.with_game(|g| {
            let a = g.assignments.iter_mut().find(|a| a.location_id == id).unwrap();
            a.target = Target::Dwell { p: destination(home(), 0.0, 300.0), r: 40.0, minutes: 3.0 };
            a.kind_id = "bench_warmer".into();
            g.traps.active.push(apgo_core::traps::Trap::Slow { until_ms: i64::MAX });
        });
        let q = e.quests(0).into_iter().find(|q| q.location_id == id).unwrap();
        assert_eq!(q.headline, "Sit 6 min");
    }

    #[test]
    fn the_danger_line_joins_traps_and_the_block_once_and_is_absent_with_neither() {
        let s = |v: &[&str]| v.iter().map(|t| (*t).to_string()).collect::<Vec<_>>();
        assert_eq!(danger_line(&[], None), None);
        assert_eq!(danger_line(&s(&["Frozen!"]), None).as_deref(), Some("Frozen!"));
        assert_eq!(danger_line(&s(&["Frozen!", "Leashed"]), Some("Frozen!".into())).as_deref(), Some("Frozen!  ·  Leashed"));
        assert_eq!(danger_line(&[], Some("Zone 2 locked".into())).as_deref(), Some("Zone 2 locked"));
    }

    #[test]
    fn deleting_a_realm_stops_its_scan_and_saves_nothing_for_it() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let e = engine_with_game("scan-stop");
        e.save_realm("r9".into(), "R9".into(), None, Some(circle(home(), 2000.0)), vec![], false).unwrap();
        let calls = AtomicUsize::new(0);
        let fetch = |_: &str, _: usize, _: Option<std::time::Instant>| -> Result<String, apgo_core::overpass::Error> {
            if calls.fetch_add(1, Ordering::SeqCst) == 0 {
                e.delete_realm("r9".into()).unwrap(); // deleted while its first request runs
            }
            Ok(r#"{"elements":[]}"#.to_string())
        };
        let one_at_a_time = Pacing { workers: 1, gap: std::time::Duration::ZERO, rounds: 1, backoff: std::time::Duration::ZERO };
        let r = e.scan_realm_with("r9", 0, &|_, _| {}, &|| true, &fetch, &one_at_a_time);
        assert!(r.is_err(), "a stopped scan has no result");
        assert_eq!(calls.load(Ordering::SeqCst), 1, "no request after the delete");
        assert!(e.store().load_atlas("r9").is_none(), "no scan saved for a deleted realm");
        assert!(e.store().get("r9").is_none());
    }

    #[test]
    fn a_scan_the_app_no_longer_wants_makes_no_request_and_saves_nothing() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let e = engine_with_game("scan-unwanted");
        e.save_realm("r9".into(), "R9".into(), None, Some(circle(home(), 2000.0)), vec![], false).unwrap();
        let calls = AtomicUsize::new(0);
        let fetch = |_: &str, _: usize, _: Option<std::time::Instant>| -> Result<String, apgo_core::overpass::Error> {
            calls.fetch_add(1, Ordering::SeqCst);
            Ok(r#"{"elements":[]}"#.to_string())
        };
        let one_at_a_time = Pacing { workers: 1, gap: std::time::Duration::ZERO, rounds: 1, backoff: std::time::Duration::ZERO };
        // The realm was hidden for undo before the scan got going: the app's listener says stop from the start.
        let r = e.scan_realm_with("r9", 0, &|_, _| {}, &|| false, &fetch, &one_at_a_time);
        assert!(r.is_err());
        assert_eq!(calls.load(Ordering::SeqCst), 0, "no request at all");
        assert!(e.store().load_atlas("r9").is_none());
        assert!(e.store().get("r9").is_some(), "the realm itself is untouched (undo can bring it back)");
    }

    #[test]
    fn each_scan_of_a_realm_has_its_own_stop_flag() {
        use std::sync::atomic::Ordering;
        let reg = ScanRegistry::default();
        let (a, b) = (reg.begin("r1"), reg.begin("r1"));
        reg.end("r1", &a); // the first scan finishes while the second still runs
        reg.cancel("r1");
        assert!(b.load(Ordering::SeqCst), "the running scan is still stopped by a cancel");
        assert!(!a.load(Ordering::SeqCst), "a finished scan is left alone");
        reg.end("r1", &b);
        reg.cancel("r1"); // nothing running: nothing to do
    }

    #[test]
    fn editing_a_zone_realm_of_the_open_game_moves_its_zone() {
        let e = engine_with_game("realm-edit");
        let p = home();
        e.on_fix(fix_at(p, FIX_T_MS + 1_000), None, false);
        assert_eq!(e.last_zone_proximity(), "inside");
        let far = destination(home(), 0.0, 50_000.0);
        e.save_realm("r0".into(), "R0".into(), None, Some(circle(far, 2000.0)), vec![], false).unwrap();
        e.on_fix(fix_at(p, FIX_T_MS + 2_000), None, false);
        assert_eq!(e.last_zone_proximity(), "far");
    }

    #[test]
    fn presence_proximity_comes_from_the_fixs_estimate_and_from_the_raw_fix_only_before_the_first() {
        // Ruling E2 (was the app's ZoneFix, review S3): judged in the same pass as the fix, from what the filter made of it.
        let e = engine_with_game("proximity-estimate");
        let far = destination(home(), 0.0, 50_000.0);
        e.on_fix(FixIn { accuracy_m: 1000.0, ..fix_at(far, FIX_T_MS) }, None, false);
        assert_eq!(e.last_zone_proximity(), "far", "an unusable first fix places nobody, but presence still learns roughly where it was");
        e.on_fix(fix_at(home(), FIX_T_MS + 1_000), None, false);
        assert_eq!(e.last_zone_proximity(), "inside");
        e.on_fix(fix_at(far, FIX_T_MS + 2_000), None, false); // 50 km in a second: gated, the estimate stays home
        assert_eq!(e.last_zone_proximity(), "inside", "a jump the filter rejects never moves presence");
    }

    #[test]
    fn deleting_a_zone_realm_of_the_open_game_drops_its_zone() {
        let e = engine_with_game("realm-delete");
        let p = home();
        e.on_fix(fix_at(p, FIX_T_MS + 1_000), None, false);
        assert_eq!(e.last_zone_proximity(), "inside");
        e.delete_realm("r0".into()).unwrap();
        e.on_fix(fix_at(p, FIX_T_MS + 2_000), None, false);
        assert_eq!(e.last_zone_proximity(), "unknown", "the deleted outline no longer counts");
    }

    #[test]
    fn a_point_is_in_no_park_without_parks_or_a_game() {
        let e = engine_with_game("park-at");
        assert!(e.quests(0).iter().all(|q| q.shape != "area"), "the test game has no parks");
        assert_eq!(e.park_at(home().lat, home().lon, 0), None);
        let d = std::env::temp_dir().join(format!("apgo-ffi-no-game-{}", std::process::id()));
        assert_eq!(Engine::new(d.to_string_lossy().into_owned()).park_at(0.0, 0.0, 0), None);
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn the_open_games_quest_text_follows_the_unit_setting() {
        let e = engine_with_game("units");
        let detail = || e.quests(0).into_iter().map(|q| q.detail).find(|d| d.starts_with("Get within")).expect("a reach quest");
        assert!(detail().ends_with(" m"), "{}", detail());
        e.set_region("US".into());
        assert!(detail().ends_with(" ft"), "{}", detail());
        e.set_unit_choice(crate::settings::UnitChoice::Metric).unwrap();
        assert!(detail().ends_with(" m"), "{}", detail());
    }

    #[test]
    fn a_test_engine_removes_its_directory() {
        let dir = {
            let e = engine_with_game("cleanup");
            assert!(e.dir.exists());
            e.dir.clone()
        };
        assert!(!dir.exists());
    }

    #[test]
    fn the_server_report_lists_every_finished_location_and_the_goal() {
        let e = engine_with_game("server-report"); // its seed name is "s"
        let r = e.server_report("s".into()).unwrap();
        assert!(r.checks.is_empty() && !r.goal, "nothing done yet");
        e.with_game(|g| {
            g.done.insert(7);
            g.done.insert(3);
            g.goal_reported = true;
        });
        let r = e.server_report("s".into()).unwrap();
        assert_eq!((r.checks, r.goal), (vec![3, 7], true), "all of them, every time: checks made offline reach the server on connect");
        e.close_game();
        assert!(e.server_report("s".into()).is_none(), "no open game: nothing to report");
    }

    #[test]
    fn a_game_only_reports_to_the_server_seed_and_slot_it_was_started_on() {
        let e = engine_with_game("server-match");
        e.with_game(|g| {
            g.done.insert(3);
            g.goal_reported = true;
        });
        assert_eq!(e.ap_server_match("s".into()), ServerMatch::Same);
        assert_eq!(e.ap_server_match("other-seed/Tester".into()), ServerMatch::Other);
        assert!(e.server_report("other-seed/Tester".into()).is_none(), "another seed's server never gets this game's checks or goal");
        e.with_game(|g| g.seed_name = "archipelago".into()); // started before games remembered their server
        assert_eq!(e.ap_server_match("s".into()), ServerMatch::Unknown);
        assert!(e.server_report("s".into()).is_none(), "an unknown server gets no automatic re-send");
        e.close_game();
        assert_eq!(e.ap_server_match("s".into()), ServerMatch::Unknown, "no game open");
    }

    // Start an Archipelago game "ap" on `e`'s realm from its open game's slot_data, connected at `address` as `Tester`.
    fn start_ap_game(e: &Engine, address: &str) {
        let slot = serde_json::to_string(&e.with_game(|g| g.slot.clone()).unwrap()).unwrap();
        e.start_archipelago(
            "ap".into(),
            "AP".into(),
            slot,
            "seed/Tester".into(),
            vec!["r0".into()],
            1,
            "any".into(),
            false,
            true,
            address.into(),
            "Tester".into(),
        )
        .unwrap();
    }

    #[test]
    fn an_archipelago_game_remembers_its_server_across_a_restart() {
        let e = engine_with_game("ap-saved");
        assert_eq!(e.ap_saved_server(), None, "a solo game has no server");
        start_ap_game(&e, "localhost:38281");
        let saved = Some(ApServerOut { address: "localhost:38281".into(), slot: "Tester".into() });
        assert_eq!(e.ap_saved_server(), saved);
        let restarted = Engine::new(e.dir.to_string_lossy().into_owned());
        assert_eq!(restarted.ap_saved_server(), None, "no game open yet");
        restarted.open_game("ap".into()).unwrap();
        assert_eq!(restarted.ap_saved_server(), saved, "the app reconnects there without the player");
    }

    #[test]
    fn a_game_from_before_saved_servers_has_none() {
        let e = engine_with_game("ap-legacy");
        e.with_game(|g| g.backend = Backend::Archipelago);
        assert_eq!(e.ap_saved_server(), None, "connect from New Game as before");
        assert!(e.set_ap_address("localhost:1".into()).is_err(), "no slot known to keep");
    }

    #[test]
    fn an_opened_game_connects_to_its_own_server_unless_already_there() {
        let e = engine_with_game("ap-target");
        let at = |a: &str, s: &str| Some(ApServerOut { address: a.into(), slot: s.into() });
        assert_eq!(e.ap_server_to_connect(None), None, "a solo game connects nowhere");
        start_ap_game(&e, "localhost:38281");
        assert_eq!(e.ap_server_to_connect(None), at("localhost:38281", "Tester"), "no session (app start): its own server");
        assert_eq!(e.ap_server_to_connect(at("localhost:38281", "Tester")), None, "already there: keep the session");
        assert_eq!(e.ap_server_to_connect(at("other:1", "Tester")), at("localhost:38281", "Tester"), "another game's server: switch");
        assert_eq!(e.ap_server_to_connect(at("localhost:38281", "Bob")), at("localhost:38281", "Tester"), "same server, other slot");
        e.close_game();
        assert_eq!(e.ap_server_to_connect(at("other:1", "Tester")), None, "no game open: leave the session alone");
    }

    #[test]
    fn a_moved_room_keeps_the_game_on_a_new_port() {
        let e = engine_with_game("ap-move");
        assert!(e.set_ap_address("localhost:1".into()).is_err(), "a solo game has no server");
        start_ap_game(&e, "archipelago.gg:38281");
        assert!(e.set_ap_address("   ".into()).is_err(), "an empty address is refused");
        e.set_ap_address(" archipelago.gg:49152 ".into()).unwrap();
        let moved = Some(ApServerOut { address: "archipelago.gg:49152".into(), slot: "Tester".into() });
        assert_eq!(e.ap_saved_server(), moved);
        assert_eq!(e.ap_server_match("seed/Tester".into()), ServerMatch::Same, "same seed and slot: same game");
        let restarted = Engine::new(e.dir.to_string_lossy().into_owned());
        restarted.open_game("ap".into()).unwrap();
        assert_eq!(restarted.ap_saved_server(), moved, "saved");
    }

    #[test]
    fn a_rename_during_a_street_swap_is_not_undone() {
        let e = engine_with_game("rename-race");
        let (id, zones) = e.streets_target(Some("r0")).unwrap();
        let streets = Game::build_streets(&e.zone_realm_atlases(&zones).iter().map(|(_, a)| a).collect::<Vec<_>>());
        let generation = e.next_streets_gen();
        // The swap reads the realm files, then a rename lands before it takes the game lock.
        let edits = e.realm_edits.load(std::sync::atomic::Ordering::SeqCst);
        let stale = e.zone_realm_files(&zones);
        e.save_realm("r0".into(), "North".into(), None, Some(circle(home(), 2000.0)), vec![], false).unwrap();
        assert!(e.swap_streets_from(&id, &zones, streets, generation, &[], stale, edits));
        assert_eq!(e.zones().into_iter().map(|z| z.realm_name).collect::<Vec<_>>(), ["North"], "the rename stays");
    }

    #[test]
    fn zone_realm_names_are_kept_with_the_game_and_follow_engine_edits() {
        let e = engine_with_game("zone-names");
        let names = || e.zones().into_iter().map(|z| z.realm_name).collect::<Vec<_>>();
        assert_eq!(names(), ["R0"]);
        // Written behind the engine's back: zones() no longer reads the realm file on every call.
        let mut r = e.store().get("r0").unwrap();
        r.name = "Behind".into();
        e.store().save(&r).unwrap();
        assert_eq!(names(), ["R0"], "zones() reads no file");
        e.save_realm("r0".into(), "Renamed".into(), None, Some(circle(home(), 2000.0)), vec![], false).unwrap();
        assert_eq!(names(), ["Renamed"], "a rename through the engine shows at once");
        e.delete_realm("r0".into()).unwrap();
        assert_eq!(names(), [""], "a deleted realm has no name");
    }

    #[test]
    fn editing_another_realm_leaves_the_zones_alone() {
        let e = engine_with_game("other-edit");
        e.save_realm("r1".into(), "R1".into(), None, Some(circle(destination(home(), 0.0, 50_000.0), 2000.0)), vec![], false).unwrap();
        let p = home();
        e.on_fix(fix_at(p, FIX_T_MS + 1_000), None, false);
        assert_eq!(e.last_zone_proximity(), "inside");
    }

    /// A fix time in the core's sane range (after 2000-01-01, adversarial review M1).
    const FIX_T_MS: i64 = 1_800_000_005_000;

    fn journaled(e: &Engine) -> usize {
        e.journal.as_ref().unwrap().lock().unwrap().track("g1", 0, i64::MAX).unwrap().len()
    }

    #[test]
    fn with_a_home_pin_a_wifi_drop_only_checks_and_fixes_at_home_are_never_recorded() {
        // The owner's phone drops home Wi-Fi for minutes: GPS may look once, but nothing at home reaches the game, journal or trace.
        let e = engine_with_game("homewatch");
        assert!(e.presence_circle().is_none(), "no home pin yet: nothing to draw");
        e.set_home(gp(home())).unwrap();
        assert!(e.presence_circle().is_none(), "no step counter yet");
        e.presence_set_walk_sensor(true);
        let drawn = e.presence_circle().unwrap();
        assert_eq!((pt(&drawn.center), drawn.radius_m), (home(), apgo_core::home_watch::HOME_CIRCLE_M), "the map draws the circle in use");
        e.presence_wifi(Some(true), FIX_T_MS);
        e.presence_start(FIX_T_MS);
        assert_eq!(e.presence_state(), PresenceHome::Home);
        e.presence_wifi(None, FIX_T_MS + 1000);
        assert_eq!((e.presence_state(), e.presence_due_ms()), (PresenceHome::Checking, Some(FIX_T_MS + 1000 + apgo_core::home_watch::CHECK_MS)));
        e.on_fix(fix_at(destination(home(), 90.0, 10.0), FIX_T_MS + 2000), None, false);
        assert_eq!(e.presence_state(), PresenceHome::Home);
        assert_eq!(journaled(&e), 0, "no point at home");
        assert!(e.trace_matched().runs.iter().all(|r| r.points.is_empty()), "no trace at home");
    }

    #[test]
    fn a_walk_out_of_the_home_circle_leaves_and_records_from_there() {
        let e = engine_with_game("homewalk");
        e.set_home(gp(home())).unwrap();
        e.presence_set_walk_sensor(true);
        e.presence_wifi(Some(true), FIX_T_MS);
        e.presence_start(FIX_T_MS);
        for s in 1..=16 {
            e.on_steps(1000 + 2 * s, FIX_T_MS + s * 1000, None);
        }
        assert_eq!(e.presence_state(), PresenceHome::Checking, "a walk burst asks GPS to look");
        e.on_fix(fix_at(destination(home(), 90.0, 50.0), FIX_T_MS + 20_000), None, false); // 45-55 m: unclear
        assert_eq!((e.presence_state(), journaled(&e)), (PresenceHome::Checking, 0), "still in the circle: nothing recorded");
        e.on_fix(fix_at(destination(home(), 90.0, 70.0), FIX_T_MS + 40_000), None, false);
        assert_eq!(e.presence_state(), PresenceHome::Away);
        assert_eq!(e.presence_left_at_ms(), Some(FIX_T_MS + 1000), "the walk began at its first step");
        assert_eq!(journaled(&e), 1, "the first fix outside is recorded");
    }

    #[test]
    fn a_simulated_fix_never_moves_presence() {
        let e = engine_with_game("homesim");
        e.set_home(gp(home())).unwrap();
        e.presence_set_walk_sensor(true);
        e.presence_start(FIX_T_MS);
        e.on_fix(fix_at(destination(home(), 90.0, 500.0), FIX_T_MS + 1000), None, true);
        // A release build believes no simulated fix: there it is a real fix 500 m out.
        assert_eq!(e.presence_state(), if SIM_ALLOWED { PresenceHome::Checking } else { PresenceHome::Away });
        e.presence_tick(FIX_T_MS + apgo_core::home_watch::CHECK_MS);
        assert_eq!(e.presence_state(), PresenceHome::Away, "no home Wi-Fi and no clear fix: playing, as before the circle");
    }

    fn fix_at(p: Point, t_ms: i64) -> FixIn {
        FixIn { t_ms, lat: p.lat, lon: p.lon, accuracy_m: 5.0, ..fix_in("gps") }
    }

    fn fix_in(provider: &str) -> FixIn {
        FixIn {
            t_ms: FIX_T_MS,
            lat: 40.0,
            lon: -111.0,
            accuracy_m: 6.0,
            speed_mps: None,
            speed_acc_mps: None,
            bearing_deg: None,
            bearing_acc_deg: None,
            altitude_m: None,
            vertical_acc_m: None,
            provider: provider.into(),
            mock: false,
        }
    }

    #[test]
    fn the_matched_trace_of_no_game_is_empty() {
        let dir = std::env::temp_dir().join(format!("apgo-ffi-trace-{}", std::process::id()));
        let e = Engine::new(dir.to_string_lossy().into_owned());
        let t = e.trace_matched();
        assert!(t.from_ms.is_none() && t.runs.is_empty());
        let d = e.trace_matched_since(7);
        assert!(d.reset && d.cursor == 0 && !d.joins && d.append.is_empty() && d.tail.points.is_empty() && d.from_ms.is_none(), "{d:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_game_opens_without_waiting_for_its_streets_and_refreshes_swap_them_in() {
        // Owner ruling (round 3): installing never builds the street graph; `refresh_streets` (run off the main thread) builds it outside
        // the game lock and swaps it in, and a slower, older build never replaces a newer one.
        let dir = std::env::temp_dir().join(format!("apgo-ffi-graph-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let e = Engine::new(dir.to_string_lossy().into_owned());
        let center = Point::new(40.0, -111.0);
        let circle = || Some(CircleOut { center: GeoPoint { lat: center.lat, lon: center.lon }, radius_m: 3000.0 });
        e.save_realm("r0".into(), "R".into(), None, circle(), vec![], false).unwrap();
        let streets: Vec<Point> =
            (-20..=20).flat_map(|n| (-20..=20).map(move |k| destination(destination(center, 0.0, f64::from(n) * 130.0), 90.0, f64::from(k) * 130.0))).collect();
        let mut atlas = build_atlas("r0", 0, vec![], streets, &e.catalog);
        let corner = destination(center, 225.0, 1000.0);
        atlas.ways = apgo_core::loc::bench::grid_ways(corner, 11, 100.0);
        e.store().save_atlas(&atlas).unwrap();
        let o = SoloOptions {
            zone_count: 1,
            run_quests: false,
            run_share: 20,
            number_of_trips: 6,
            goal: "all_trips".into(),
            quest_types: vec!["reach".into()],
            ..SoloOptions::default()
        };
        let generated = generate(&o, 4).unwrap();
        let realms = e.realm_atlases(&["r0".into()]).unwrap();
        let new = NewGame {
            id: "g".into(),
            name: "G".into(),
            backend: Backend::Solo,
            seed_name: "s".into(),
            slot: generated.slot,
            zone_realms: vec!["r0".into()],
            realms: &realms,
            home: center,
            seed: 4,
            solo_rewards: generated.rewards,
            surface: SurfacePref::Any,
            avoid_stairs: false,
            allow_parking: true,
        };
        e.install(Game::create(new, &e.catalog).unwrap());
        let graph = |e: &Engine| e.game.lock().unwrap().as_ref().and_then(|g| g.street_graph().map(|g| g.segment_count()));
        assert_eq!(graph(&e), None, "the game is open at once, without its graph");
        assert!(e.last_accepted_pos().is_none(), "no fix yet");
        e.on_fix(fix_in("gps"), None, false);
        assert!(e.position(5_000).is_some(), "a fix is processed normally without the graph");
        // Adversarial review M3: logic (traps placed by Archipelago items) takes the last accepted position, not the pin.
        assert!(e.last_accepted_pos().is_some_and(|p| (p.lat - 40.0).abs() < 1e-9 && (p.lon + 111.0).abs() < 1e-9));
        e.set_counting(false, 0);
        assert!(e.last_accepted_pos().is_some_and(|p| (p.lat - 40.0).abs() < 1e-9), "kept across a pause (re-review N5)");
        e.set_counting(true, 0);
        let built = e.refresh_streets(None).unwrap();
        assert_eq!((built.segments, built.degraded, graph(&e)), (220, false, Some(220)), "the refresh swapped the graph in");
        assert!(e.refresh_streets(Some("other".into())).is_none(), "another realm's edit leaves the game alone");
        // Two rebuilds finishing out of order: the older generation is dropped.
        let (older_gen, newer_gen) = (e.next_streets_gen(), e.next_streets_gen());
        let (id, zones) = e.streets_target(Some("r0")).unwrap();
        let older = Game::build_streets(&e.zone_realm_atlases(&zones).iter().map(|(_, a)| a).collect::<Vec<_>>());
        atlas.ways = apgo_core::loc::bench::grid_ways(corner, 6, 100.0);
        e.store().save_atlas(&atlas).unwrap(); // a rescan in between
        let newer = Game::build_streets(&e.zone_realm_atlases(&zones).iter().map(|(_, a)| a).collect::<Vec<_>>());
        assert!(e.swap_streets(&id, &zones, newer, newer_gen, &[]));
        assert!(!e.swap_streets(&id, &zones, older, older_gen, &[]), "the older build finished last but is not swapped in");
        assert_eq!(graph(&e), Some(2 * 6 * 5));
        // Final review M2: an outline edit refreshes the zone shapes (in-zone, proximity, the filter's mode) with the streets.
        assert_eq!(e.zone_distance_m(center), Some(0.0));
        let moved = destination(center, 90.0, 20_000.0);
        let moved_circle = Some(CircleOut { center: GeoPoint { lat: moved.lat, lon: moved.lon }, radius_m: 3000.0 });
        e.save_realm("r0".into(), "R".into(), None, moved_circle, vec![], false).unwrap();
        assert!(e.refresh_streets(Some("r0".into())).is_some());
        assert!(e.zone_distance_m(center).is_some_and(|d| d > 10_000.0), "{:?}", e.zone_distance_m(center));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_fix_with_every_option_missing_is_a_plain_fix_and_simulated_ones_are_sim() {
        let r = raw_fix(&fix_in("gps"), false, true);
        assert_eq!((r.t_ms, r.provider, r.speed_mps, r.bearing_acc_deg), (FIX_T_MS, Provider::Gps, None, None));
        assert_eq!(raw_fix(&fix_in("fused"), true, true).provider, Provider::Sim);
        assert_eq!(raw_fix(&fix_in("weird"), false, true).provider, Provider::Other);
    }

    #[test]
    fn only_the_simulated_argument_of_a_debug_build_makes_a_sim_fix() {
        // Adversarial review I3: a Location named "sim" skipped the mock, accuracy, NaN and time checks. The name is no sim fix, and
        // the argument counts only where simulation is allowed (debug builds of the core).
        assert_eq!(raw_fix(&fix_in("sim"), false, true).provider, Provider::Other);
        assert_eq!(raw_fix(&fix_in("sim"), true, false).provider, Provider::Other, "a release build has no simulator");
        assert_eq!(raw_fix(&fix_in("gps"), true, false).provider, Provider::Gps);
        assert_eq!(SIM_ALLOWED, cfg!(debug_assertions));
    }

    #[test]
    fn screen_off_fixes_batched_late_behind_steps_are_journaled() {
        // Walk test 2026-10-10: locked, a fix came every ~13 s, handed over after 8 s of newer steps; none was journaled or drawn.
        let e = engine_with_game("screen-off");
        let at = |t_s: i64| fix_at(destination(home(), 90.0, 1.4 * f64::from(i32::try_from(t_s).unwrap())), FIX_T_MS + t_s * 1000);
        let steps_until = |from_s: i64, to_s: i64| {
            for t in (from_s..to_s).filter(|t| t % 2 == 0) {
                e.on_steps(10_000 + t * 193 / 100, FIX_T_MS + t * 1000, None);
            }
        };
        steps_until(0, 1);
        for t in 0..20 {
            e.on_fix(at(t), None, false);
        }
        e.set_fix_spacing_hint(Some(15_000));
        let sparse: Vec<i64> = (32..=200).step_by(13).collect();
        for &tf in &sparse {
            steps_until(tf - 12, tf + 9);
            e.on_fix(at(tf), None, false);
        }
        let journaled: Vec<i64> =
            e.journal.as_ref().unwrap().lock().unwrap().track("g1", FIX_T_MS + 30_000, FIX_T_MS + 201_000).unwrap().iter().map(|p| p.t_ms).collect();
        let missing: Vec<i64> = sparse.iter().filter(|tf| !journaled.contains(&(FIX_T_MS + *tf * 1000))).copied().collect();
        assert!(missing.is_empty(), "not journaled: {missing:?}");
    }

    #[test]
    fn journal_points_are_throttled_to_five_seconds_or_five_metres() {
        let p = |t_ms: i64, east_m: f64| {
            let q = destination(Point::new(40.0, -111.0), 90.0, east_m);
            TrackPoint { t_ms, lat: q.lat, lon: q.lon, accuracy_m: 4.0, simulated: false }
        };
        assert!(journal_due(None, &p(0, 0.0)));
        assert!(!journal_due(Some(&p(0, 0.0)), &p(4_000, 3.0)));
        assert!(journal_due(Some(&p(0, 0.0)), &p(5_000, 0.0)));
        assert!(journal_due(Some(&p(0, 0.0)), &p(1_000, 6.0)));
    }

    #[test]
    fn the_log_throttles_restart_when_the_simulator_clock_falls_back_to_real_time() {
        // Controller note 1: the simulator's clock runs ahead, so the first real fix after it looks 10 minutes older than the last point.
        let dir = std::env::temp_dir().join(format!("apgo-ffi-throttle-{}", std::process::id()));
        let e = Engine::new(dir.to_string_lossy().into_owned());
        let p = |t_ms: i64, simulated: bool| TrackPoint { t_ms, lat: 40.0, lon: -111.0, accuracy_m: 3.0, simulated };
        let t = 1_000_000;
        assert!(e.journal_point_due(&p(t + 600_000, true)) && e.reject_log_due(t + 600_000));
        assert!(e.journal_point_due(&p(t, false)), "the first real point after the simulator is written");
        assert!(e.reject_log_due(t), "a rejected real fix is not muted by the simulator's clock");
        assert!(!e.journal_point_due(&p(t + 1_000, false)) && !e.reject_log_due(t + 1_000), "then the throttles hold again");
        assert!(journal_due(Some(&p(t, false)), &p(t + 1_000, true)), "switching to the simulator writes a point at once");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn rejected_fixes_are_described_by_verdict() {
        use apgo_core::loc::{Estimate, Verdict};
        let e = |verdict| Estimate { verdict, accepted: false, uncertainty_m: 41.0, ..Estimate::exact(40.0, -111.0, 0) };
        assert_eq!(rejected_detail(&e(Verdict::Blurry), UnitSystem::Metric).as_deref(), Some("uncertain 45 m"));
        assert_eq!(rejected_detail(&e(Verdict::Gated), UnitSystem::Metric).as_deref(), Some("ignored as a GPS jump"));
        assert_eq!(rejected_detail(&e(Verdict::Unusable), UnitSystem::Metric).as_deref(), Some("unusable fix"));
        assert_eq!(rejected_detail(&e(Verdict::Reset), UnitSystem::Metric).as_deref(), Some("uncertain 45 m"), "a coarse restart (ruling FR-I1)");
        assert_eq!(rejected_detail(&e(Verdict::Relocated), UnitSystem::Metric).as_deref(), Some("uncertain 45 m"));
        assert_eq!(rejected_detail(&Estimate { verdict: Verdict::Reset, ..Estimate::exact(40.0, -111.0, 0) }, UnitSystem::Metric), None);
        assert_eq!(rejected_detail(&Estimate::exact(40.0, -111.0, 0), UnitSystem::Metric), None);
        // In the player's units, rounded the safe way (up) as every distance the core writes.
        assert_eq!(rejected_detail(&e(Verdict::Blurry), UnitSystem::Imperial).as_deref(), Some("uncertain 140 ft"));
    }

    #[test]
    fn position_before_any_fix_is_none_and_headings_without_a_game_are_ignored() {
        let dir = std::env::temp_dir().join(format!("apgo-ffi-pos-{}", std::process::id()));
        let e = Engine::new(dir.to_string_lossy().into_owned());
        assert!(e.position(0).is_none());
        e.on_heading(HeadingIn { azimuth_deg: 10.0, accuracy: "high".into(), pitch_deg: 0.0, roll_deg: 0.0, t_ms: 1, error_deg: None });
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_heading_maps_to_the_core_with_its_error() {
        let h = HeadingIn { azimuth_deg: 10.0, accuracy: "low".into(), pitch_deg: 1.0, roll_deg: 2.0, t_ms: 3, error_deg: Some(12.0) };
        let c = heading_in(&h);
        assert_eq!((c.t_ms, c.azimuth_deg, c.accuracy, c.pitch_deg, c.roll_deg), (3, 10.0, apgo_core::loc::CompassAccuracy::Low, 1.0, 2.0));
        assert_eq!((c.error_deg, heading_in(&HeadingIn { error_deg: None, ..h }).error_deg), (Some(12.0), None));
    }

    #[test]
    fn a_display_position_maps_to_its_ffi_record() {
        use apgo_core::loc::{DisplayPosition, DisplaySource, HeadingSource};
        let d = DisplayPosition {
            lat: 1.0,
            lon: 2.0,
            heading_source: HeadingSource::Compass,
            source: DisplaySource::Bridged,
            heading_deg: Some(45.0),
            ..DisplayPosition::default()
        };
        let out = position_out(&d);
        assert_eq!((out.heading_source.as_str(), out.source.as_str(), out.heading_deg), ("compass", "bridged", Some(45.0)));
        let aged = DisplayPosition { source: DisplaySource::Stale, bridged_origin: true, ..d };
        assert!(position_out(&aged).bridged_origin && !position_out(&DisplayPosition { bridged_origin: false, ..d }).bridged_origin);
        let names = |hs, s| {
            let o = position_out(&DisplayPosition { heading_source: hs, source: s, ..d });
            (o.heading_source, o.source)
        };
        assert_eq!(names(HeadingSource::Course, DisplaySource::Gps), ("course".into(), "gps".into()));
        assert_eq!(names(HeadingSource::None, DisplaySource::Predicted), ("none".into(), "predicted".into()));
        assert_eq!(names(HeadingSource::None, DisplaySource::Stale).1, "stale");
    }

    #[test]
    fn without_a_game_proximity_is_unknown_and_no_zone_is_near() {
        let dir = std::env::temp_dir().join(format!("apgo-ffi-zone-{}", std::process::id()));
        let e = Engine::new(dir.to_string_lossy().into_owned());
        assert_eq!(e.last_zone_proximity(), "unknown");
        assert_eq!(e.zone_distance_m(Point::new(40.0, -111.0)), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_step_calibration_maps_both_ways_and_needs_a_game() {
        let c = StepCalIn { source: "phone.step_counter".into(), k: 0.92, var_k: 0.0004, samples: 17, updated_ms: 1_800_000_000_000 };
        let core = step_cal_in(c.clone());
        assert_eq!((core.source.as_str(), core.k, core.var_k, core.samples, core.updated_ms), ("phone.step_counter", 0.92, 0.0004, 17, 1_800_000_000_000));
        assert!(step_cal_to_save(StepCal { samples: 0, ..core.clone() }).is_none(), "never learned: nothing to save (review M5)");
        let out = step_cal_to_save(core).unwrap();
        assert_eq!((out.source, out.k, out.var_k, out.samples, out.updated_ms), (c.source.clone(), c.k, c.var_k, c.samples, c.updated_ms));
        let dir = std::env::temp_dir().join(format!("apgo-ffi-stepcal-{}", std::process::id()));
        let e = Engine::new(dir.to_string_lossy().into_owned());
        e.set_step_calibration(c);
        assert!(e.step_calibration().is_none(), "no game, nothing to save");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_fix_without_a_game_does_nothing() {
        let dir = std::env::temp_dir().join(format!("apgo-ffi-fix-{}", std::process::id()));
        let e = Engine::new(dir.to_string_lossy().into_owned());
        assert!(e.on_fix(fix_in("gps"), None, false).is_empty());
        assert!(e.on_steps(10, 1, Some(1.8)).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_trap_alerts_only_while_trap_alerts_are_on() {
        let e = engine_with_game("trap-alert");
        let alert = |ev: Vec<EventOut>| ev.into_iter().find_map(|x| if let EventOut::Trap { alert, .. } = x { Some(alert) } else { None });
        assert!(e.trap_alerts(), "on by default");
        assert_eq!(alert(e.sync_items(vec!["Slow Trap".into()], 0, None)), Some(true));
        e.set_trap_alerts(false).unwrap();
        assert!(!e.trap_alerts());
        assert_eq!(alert(e.sync_items(vec!["Slow Trap".into(), "Fog Of War Trap".into()], 10, None)), Some(false));
    }

    #[test]
    fn a_forager_reports_its_items_and_counts_and_other_quests_report_none() {
        let (a, b) = (Point::new(40.0, -111.0), Point::new(40.01, -111.0));
        let t = Target::Collect { pts: vec![a, b], need: 1, r: 25.0, theme: "shells".into() };
        let c = Collected { picked: BTreeSet::from([0]), carried: 1, banked: 0 };
        let out = collect_out(&t, Some(&c), &[b], QuestState::InProgress).expect("a forager has items");
        assert_eq!((out.theme.as_str(), out.need, out.carried, out.banked), ("shells", 1, 1, 0));
        assert_eq!(out.items.iter().map(|i| i.picked).collect::<Vec<_>>(), [false], "only the items still on the map (#142)");
        assert!((out.items[0].at.lat - 40.01).abs() < 1e-12);
        assert!(collect_out(&Target::Point { p: a, r: 40.0 }, None, &[], QuestState::InProgress).is_none());
        assert_eq!(first_open(&out).map(|p| p.lat), Some(40.01), "the pin to open is the first item still out there");
    }

    #[test]
    fn a_done_forager_keeps_its_anchor_and_an_unfinished_one_sits_on_its_first_open_item() {
        let (a, b) = (Point::new(40.0, -111.0), Point::new(40.01, -111.0));
        let t = Target::Collect { pts: vec![a, b], need: 1, r: 25.0, theme: "shells".into() };
        let c = collect_out(&t, Some(&Collected { picked: BTreeSet::from([0]), carried: 0, banked: 1 }), &[b], QuestState::InProgress);
        let lat = |s| pin_at(c.as_ref(), s, Some(a), b).map(|p| p.lat);
        assert_eq!(lat(QuestState::Done), Some(40.0), "a done forager stays in the places list like any done quest");
        assert_eq!(lat(QuestState::InProgress), Some(40.01));
        assert_eq!(pin_at(None, QuestState::Done, Some(b), a).map(|p| p.lat), Some(40.01), "other quests keep their anchor");
    }

    #[test]
    fn only_an_unfinished_forager_with_items_out_there_draws_them_instead_of_a_pin() {
        let a = Point::new(40.0, -111.0);
        let t = Target::Collect { pts: vec![a], need: 1, r: 25.0, theme: "shells".into() };
        let on_map = |live: &[Point], s| collect_out(&t, None, live, s).is_some_and(|c| c.on_map);
        assert!(on_map(&[a], QuestState::InProgress));
        assert!(!on_map(&[a], QuestState::Done), "a done forager is drawn like any done quest");
        assert!(!on_map(&[], QuestState::InProgress), "every item picked: its pin shows");
        assert!(!on_map(&[a], QuestState::Hidden), "a fogged forager shows nothing");
    }

    #[test]
    fn a_forager_with_every_item_picked_and_some_carried_sits_at_home() {
        let (a, home) = (Point::new(40.01, -111.0), Point::new(40.0, -111.0));
        let t = Target::Collect { pts: vec![a], need: 2, r: 25.0, theme: "shells".into() };
        let carrying = collect_out(&t, Some(&Collected { picked: BTreeSet::from([0]), carried: 1, banked: 0 }), &[], QuestState::InProgress);
        assert_eq!(pin_at(carrying.as_ref(), QuestState::InProgress, Some(a), home).map(|p| p.lat), Some(40.0), "bring them home");
        let waiting = collect_out(&t, Some(&Collected { picked: BTreeSet::from([0]), carried: 0, banked: 1 }), &[], QuestState::InProgress);
        assert_eq!(pin_at(waiting.as_ref(), QuestState::InProgress, Some(a), home).map(|p| p.lat), Some(40.01), "nothing carried: its anchor");
    }
}
