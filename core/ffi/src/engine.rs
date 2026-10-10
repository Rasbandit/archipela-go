//! `UniFFI` facade over the game engine: realms, scanning, game setup, play. Blocking calls; call from a background thread.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::PathBuf;
use std::sync::atomic::AtomicU64;
use std::sync::{Arc, Mutex, MutexGuard};

use apgo_core::assign::SurfacePref;
use apgo_core::assign::Target;
use apgo_core::catalog::{Catalog, Kind, Mode};
use apgo_core::chain::ChainUnit;
use apgo_core::game::{Backend, Event, Game, NearMiss, NewGame, QuestState, Streets};
use apgo_core::geo::{distance_m, simplify_spaced, Point};
use apgo_core::journal::{kind, Journal, JournalEvent, TrackPoint, DEFAULT_MAX_GAP_MS};
use apgo_core::loc::calib::StepCal;
use apgo_core::loc::{mode_at, DisplayPosition, DisplaySource, Estimate, HeadingSource, Provider, RawFix, Verdict};
use apgo_core::marks::Mark;
use apgo_core::num::count_u32;
use apgo_core::realm::{Proximity, Realm, RealmStore, Shape};
use apgo_core::save_policy::SavePolicy;
use apgo_core::scan::{scan_realm, Atlas};
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

/// One chosen win condition and its parameter (0 means that goal's default).
#[derive(Debug, uniffi::Record)]
pub struct GoalPickIn {
    /// Goal id.
    pub id: String,
    /// Parameter of the goal; 0 means its default.
    pub target: u32,
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
    /// The win conditions (at least one).
    pub goals: Vec<GoalPickIn>,
    /// How they combine: "any", "all" or "`at_least`".
    pub goal_requirement: String,
    /// For "`at_least"`: how many of the goals must be finished.
    pub goal_need: u32,
    /// How many quest locations the game has.
    pub number_of_trips: u32,
    /// Travel mode of each zone, in zone order.
    pub zone_modes: Vec<String>,
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
    let zone_modes = o.zone_modes.iter().map(|m| Mode::parse(m).ok_or_else(|| err(format!("unknown mode {m}")))).collect::<Result<Vec<_>, _>>()?;
    let goal_mode = match o.goal_requirement.as_str() {
        "any" => apgo_core::slot::GoalMode::Any,
        "all" => apgo_core::slot::GoalMode::All,
        "at_least" => apgo_core::slot::GoalMode::AtLeast,
        other => return Err(err(format!("unknown goal requirement {other}"))),
    };
    Ok(SoloOptions {
        goal: String::new(),
        goal_target: 0,
        goals: o.goals.into_iter().map(|g| apgo_core::slot::GoalSpec { id: g.id, target: g.target }).collect(),
        goal_mode,
        goal_need: o.goal_need,
        zone_quest_types: o.zone_quest_types,
        number_of_trips: o.number_of_trips,
        zone_modes,
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
    /// How the player travels there: walk, run, bike or drive.
    pub mode: String,
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
    /// Every item, in the quest's order.
    pub items: Vec<CollectItemOut>,
}

fn collect_out(t: &Target, c: Option<&Collected>) -> Option<CollectOut> {
    let Target::Collect { pts, need, theme, .. } = t else { return None };
    let c = c.cloned().unwrap_or_default();
    let items = pts.iter().enumerate().map(|(i, p)| CollectItemOut { at: gp(*p), picked: u16::try_from(i).is_ok_and(|i| c.picked.contains(&i)) }).collect();
    Some(CollectOut { theme: theme.clone(), need: *need, carried: c.carried, banked: c.banked, items })
}

/// The first item still out there, where the quest's pin and popup sit.
fn first_open(c: &CollectOut) -> Option<Point> {
    c.items.iter().find(|i| !i.picked).map(|i| pt(&i.at))
}

/// Where a quest's pin and popup sit: an unfinished forager's first item still out there, otherwise its own anchor.
fn pin_at(collect: Option<&CollectOut>, state: QuestState, anchor: Option<Point>) -> Option<Point> {
    match collect {
        Some(c) if state != QuestState::Done => first_open(c).or(anchor),
        _ => anchor,
    }
}

/// A zone of the open game.
#[derive(Debug, uniffi::Record)]
pub struct ZoneOut {
    /// Zone number.
    pub id: u32,
    /// How the player travels in the zone: walk, run, bike or drive.
    pub mode: String,
    /// Whether the player can enter the zone.
    pub unlocked: bool,
    /// Zone keys needed to open the zone.
    pub keys_needed: u32,
    /// Name of the tool item the zone needs, if any.
    pub tool: Option<String>,
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

fn ev_out(e: Event) -> EventOut {
    match e {
        Event::QuestDone { location_id, name } => EventOut::QuestDone { location_id, name },
        Event::SendCheck { location_id } => EventOut::SendCheck { location_id },
        Event::Reward { location_id, item } => EventOut::Reward { location_id, item },
        Event::ZoneUnlocked { zone } => EventOut::ZoneUnlocked { zone },
        Event::Trap { item, message } => EventOut::Trap { item, message },
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

/// How many events of one kind happened.
#[derive(Debug, uniffi::Record)]
pub struct KindCountOut {
    /// Machine name of the event kind.
    pub kind: String,
    /// Number of events of that kind.
    pub count: u32,
}

/// "While you were out": everything recorded between two moments.
#[derive(Debug, uniffi::Record)]
pub struct AwayReportOut {
    /// Start of the period, in Unix milliseconds.
    pub from_ms: i64,
    /// End of the period, in Unix milliseconds.
    pub to_ms: i64,
    /// Number of GPS points recorded.
    pub points: u32,
    /// How many of those points came from the dev simulator.
    pub simulated_points: u32,
    /// Distance travelled in the period, in metres.
    pub distance_m: f64,
    /// Number of events of each kind.
    pub counts: Vec<KindCountOut>,
    /// The events themselves, oldest first.
    pub events: Vec<AuditEventOut>,
}

/// Quests this close to a fix get a line in the log saying whether they count.
const NEAR_MISS_RADIUS_M: f64 = 100.0;
/// Longest the open game goes unsaved while the player moves without events.
const SAVE_INTERVAL_MS: i64 = 30_000;
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

/// The travel mode for the filter on the next fix (ruling E2): the zone mode at the last estimate, at the raw fix only before the first one.
fn fix_mode(zones: &[(Shape, Mode)], last_est: Option<Point>, raw: Point) -> Option<Mode> {
    mode_at(zones, last_est.unwrap_or(raw))
}

/// Whether an accepted estimate goes into the journal: 5 s or 5 m after the last point written, or at once when the clock jumped (a switch
/// between simulated and real fixes, or a time before the last point: the simulator's clock runs ahead).
fn journal_due(last: Option<&TrackPoint>, p: &TrackPoint) -> bool {
    last.is_none_or(|l| {
        l.simulated != p.simulated || p.t_ms < l.t_ms || p.t_ms - l.t_ms >= 5_000 || distance_m(Point::new(l.lat, l.lon), Point::new(p.lat, p.lon)) >= 5.0
    })
}

/// Each of `game`'s zones as its realm's shape (from `shapes`, one per zone realm) and its travel mode; zones whose realm is gone are left
/// out.
fn zone_modes_of(game: &Game, shapes: Vec<Option<Shape>>) -> Vec<(Shape, Mode)> {
    game.slot.zones.iter().zip(shapes).filter_map(|(z, s)| s.map(|s| (s, z.mode))).collect()
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
    /// Shape and travel mode of each zone of the open game: the "inside a zone" check and the filter's mode at the player's position.
    zone_modes: Mutex<Vec<(Shape, Mode)>>,
    // Where the last fix's estimate was relative to the zones, worked out once in `on_fix` for presence to read.
    last_proximity: Mutex<Option<Proximity>>,
    /// The phone's region (ISO country code) that `Auto` units follow; empty until the app sets it.
    region: Mutex<String>,
    /// The units text is written in, worked out from the unit setting and `region` whenever either changes.
    units: Mutex<UnitSystem>,
    /// The last journal point written, for the 5 s / 5 m throttle.
    last_journal: Mutex<Option<TrackPoint>>,
    /// The last street-build generation handed out (see [`Game::set_streets`]).
    streets_gen: AtomicU64,
}

impl Engine {
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

    fn store(&self) -> RealmStore {
        RealmStore::new(&self.dir)
    }

    fn cache(&self) -> PathBuf {
        self.dir.join("http-cache")
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
    fn zone_atlases(&self, zone_realms: &[String]) -> Vec<Atlas> {
        let store = self.store();
        zone_realms.iter().collect::<BTreeSet<_>>().into_iter().filter_map(|id| store.get(id)).filter_map(|r| self.zoned_atlas(&r)).collect()
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
    fn swap_streets(&self, game_id: &str, zone_realms: &[String], streets: Streets, generation: u64) -> bool {
        let shapes = self.realm_shapes(zone_realms); // read before the lock: no file reads while fixes wait
        let mut slot = self.game.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let Some(g) = slot.as_mut().filter(|g| g.id == game_id && g.zone_realms == zone_realms) else { return false };
        if !g.set_streets(streets, generation) {
            return false;
        }
        // Set while the game lock is held, so a game installed meanwhile cannot get these shapes (lock order: game, then zones).
        let modes = zone_modes_of(g, shapes);
        self.set_zones(slot, modes);
        true
    }

    /// The saved shape of each realm in `ids` (`None` for one that is gone).
    fn realm_shapes(&self, ids: &[String]) -> Vec<Option<Shape>> {
        let store = self.store();
        ids.iter().map(|id| store.get(id).map(|r| r.shape)).collect()
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
        let shapes = self.realm_shapes(&game.zone_realms);
        let mut slot = self.game.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        // Read the units under the game lock: `refresh_units` stores new units before it takes this lock, so a change racing
        // this install is either seen here or applied to this game right after.
        game.set_units(self.unit_system());
        // Keep the outgoing game's progress (unless the new one replaces that very save).
        if let Some(old) = slot.as_ref().filter(|old| old.id != game.id) {
            if let Err(e) = old.save(&self.dir) {
                self.note(format!("could not save game {} before replacing it: {e}", old.id));
            }
        }
        let modes = zone_modes_of(&game, shapes);
        *slot = Some(game);
        self.set_zones(slot, modes);
    }

    /// Realm `id` was redrawn or deleted: when it is a zone of the open game, in-zone checks use what is saved now, not what was
    /// saved when the game was opened.
    fn refresh_zones_of(&self, id: &str) {
        let slot = self.game.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let Some(modes) = slot.as_ref().filter(|g| g.zone_realms.iter().any(|r| r == id)).map(|g| zone_modes_of(g, self.realm_shapes(&g.zone_realms))) else {
            return;
        };
        self.set_zones(slot, modes);
    }

    // The open game's zone shapes; a new set (another game, or none) forgets where the last fix was relative to the old ones.
    // Takes the game guard and releases it after, so the shapes only ever change together with the game, never between another
    // thread's read and write (lock order: game, then zones).
    fn set_zones(&self, game: MutexGuard<'_, Option<Game>>, modes: Vec<(Shape, Mode)>) {
        *self.zone_modes.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = modes;
        *self.last_proximity.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = None;
        drop(game);
    }

    /// Distance in metres from a point to the nearest zone area of the open game (0 inside), or `None` with no game.
    fn zone_distance_m(&self, p: Point) -> Option<f64> {
        let zones = self.zone_modes.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        zones.iter().map(|(s, _)| s.distance_m(p)).reduce(f64::min)
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
            let Some(kind) = self.catalog.kind(kind_id).filter(|k| Mode::PLAY.iter().any(|&m| k.allows(m))) else { continue };
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
            .offers(&self.catalog, &Mode::PLAY)
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
            zone_modes: Mutex::new(Vec::new()),
            last_proximity: Mutex::new(None),
            units: Mutex::new(resolve_units(Settings::load(&dir).units, "")),
            region: Mutex::new(String::new()),
            last_journal: Mutex::new(None),
            streets_gen: AtomicU64::default(),
            dir,
        })
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
        let streets = Game::build_streets(&self.zone_atlases(&zone_realms).iter().collect::<Vec<_>>());
        let build_ms = u32::try_from(started.elapsed().as_millis()).unwrap_or(u32::MAX);
        let (segments, degraded) = streets.1.as_ref().map_or((0, false), |g| (count_u32(g.segment_count()), g.is_degraded()));
        self.swap_streets(&game_id, &zone_realms, streets, generation).then_some(StreetsBuiltOut { build_ms, segments, degraded })
    }

    /// Delete a realm with its scan and marks.
    ///
    /// # Errors
    /// Returns an error if a file cannot be removed.
    pub fn delete_realm(&self, id: String) -> Result<(), CoreError> {
        self.store().delete(&id).map_err(err)?;
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
        let store = self.store();
        let realm = store.get(&id).ok_or_else(|| err("realm not found"))?;
        let atlas =
            scan_realm(&realm, &self.catalog, Some(&self.cache()), now_ms, &|done, total| listener.progress(count_u32(done), count_u32(total))).map_err(err)?;
        store.save_atlas(&atlas).map_err(err)?;
        // The realm may have been edited while the scan ran: stamp the scan time on its current state, not on the copy read before.
        let mut current = store.get(&id).ok_or_else(|| err("realm was deleted during the scan"))?;
        current.scanned_at_ms = Some(now_ms);
        store.save(&current).map_err(err)?;
        let mut zoned = atlas;
        zoned.restrict_to(&current.shape.to_zone());
        zoned.apply_marks(&store.marks(&id));
        Ok(self.offers_of(&zoned))
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
        self.store().set_home(pt(&p)).map_err(err)
    }

    // ---------- game setup ----------
    /// Archipelago player YAML for `player` from the solo options.
    ///
    /// # Errors
    /// Returns an error if a travel mode or the goal requirement is unknown.
    pub fn build_yaml(&self, player: String, o: SoloOptionsIn) -> Result<String, CoreError> {
        Ok(build_yaml(&player, &to_core(o)?))
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
    ) -> Result<(), CoreError> {
        let opts = to_core(o)?;
        if zone_realms.len() != opts.zone_modes.len() {
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
    ) -> Result<(), CoreError> {
        let slot = SlotData::from_json(&slot_json).map_err(err)?;
        let realms = self.realm_atlases(&zone_realms)?;
        let home = self.home_for(&realms);
        let game = Game::create(
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
            },
            &self.catalog,
        )
        .map_err(err)?;
        game.save(&self.dir).map_err(err)?;
        self.install(game);
        Ok(())
    }

    /// The zone modes a connected Archipelago game needs, so the app can ask for matching realms.
    /// The travel mode of each zone in `slot_json`.
    ///
    /// # Errors
    /// Returns an error if the `slot_data` is invalid.
    pub fn slot_zone_modes(&self, slot_json: String) -> Result<Vec<String>, CoreError> {
        Ok(SlotData::from_json(&slot_json).map_err(err)?.zones.iter().map(|z| z.mode.name().to_string()).collect())
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
        self.set_zones(game, Vec::new());
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
                self.set_zones(g, Vec::new());
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
            g.quest_views(now_ms)
                .into_iter()
                .map(|q| {
                    let (shape, anchor, anchor_b, radius_m, path, detail) = describe(&q.target, units);
                    let collect = collect_out(&q.target, q.collected.as_ref());
                    let anchor = pin_at(collect.as_ref(), q.state, anchor);
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
                    }
                })
                .collect()
        })
        .unwrap_or_default()
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
        let store = self.store();
        self.with_game(|g| {
            g.slot
                .zones
                .iter()
                .enumerate()
                .map(|(i, z)| ZoneOut {
                    id: z.id,
                    mode: z.mode.name().into(),
                    unlocked: g.zone_unlocked(z.id),
                    keys_needed: z.zone_keys_needed,
                    tool: z.tool.clone(),
                    realm_name: g.zone_realms.get(i).and_then(|id| store.get(id)).map(|r| r.name).unwrap_or_default(),
                    realm_id: g.zone_realms.get(i).cloned().unwrap_or_default(),
                })
                .collect()
        })
        .unwrap_or_default()
    }

    /// What the Play screen shows, at `now_ms`; `None` with no game open.
    pub fn hud(&self, now_ms: i64) -> Option<HudOut> {
        self.with_game(|g| {
            let s = g.goal_status(now_ms);
            let views = g.quest_views(now_ms);
            let mut letters: Vec<char> = g.items.iter().filter_map(|i| i.strip_prefix("Letter ").and_then(|s| s.chars().next())).collect();
            letters.sort_unstable();
            let tools: Vec<String> = ["Running Shoes", "Bike", "Car"].iter().filter(|t| g.items.iter().any(|i| i == *t)).map(ToString::to_string).collect();
            HudOut {
                away_running: g.away_running(),
                goals: g
                    .goal_statuses(now_ms)
                    .into_iter()
                    .map(|(spec, st)| GoalLineOut { id: spec.id, label: st.label, progress: st.progress, achieved: st.achieved })
                    .collect(),
                goal_label: s.label,
                goal_progress: s.progress,
                goal_achieved: s.achieved,
                done: count_u32(views.iter().filter(|v| v.state == QuestState::Done).count()),
                total: count_u32(views.len()),
                keys: count_u32(g.items.iter().filter(|i| *i == "Progressive Zone Key").count()),
                tools,
                letters: letters.into_iter().collect(),
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
        let Some((game_id, ev, entries, near, est)) = self.with_game(|g| {
            // Ruling E2: the mode (set before the filter runs) and the zone come from estimates, the raw fix only before the first one.
            let mode = fix_mode(&self.zone_modes.lock().unwrap_or_else(std::sync::PoisonError::into_inner), g.last_estimate().map(|e| e.point()), raw.point());
            if let Some(m) = mode {
                g.set_travel_mode(m);
            }
            // One pass over the zone shapes per fix, at this fix's own estimate: the proximity presence reads next.
            let ev = g.on_fix_with_zone(&raw, steps, |p| self.set_proximity(self.zone_distance_m(p)));
            // No estimate yet: the fix was unusable and places nobody (presence still learns roughly where it was).
            let est = g.last_estimate();
            if est.is_none() {
                self.set_proximity(self.zone_distance_m(raw.point()));
            }
            self.save_if_due(g, t_ms, !ev.is_empty());
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
        self.journal_do(|j| {
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
        });
        ev.into_iter().map(ev_out).collect()
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
        let Some((game_id, ev, entries)) = self.with_game(|g| {
            let ev = g.on_steps(total, t_ms, cadence);
            self.save_if_due(g, t_ms, !ev.is_empty());
            let entries = g.journal_events(&ev, t_ms, None);
            (g.id.clone(), ev, entries)
        }) else {
            return Vec::new();
        };
        self.journal_do(|j| entries.iter().try_for_each(|e| j.log(&game_id, e)));
        ev.into_iter().map(ev_out).collect()
    }

    /// The phone joined home Wi-Fi: every forager quest banks what it carries (see `Game::bank_at_home`). Safe to call again.
    pub fn bank_at_home(&self, t_ms: i64) -> Vec<EventOut> {
        let Some((game_id, ev, entries)) = self.with_game(|g| {
            let ev = g.bank_at_home(t_ms);
            self.save_if_due(g, t_ms, true);
            let entries = g.journal_events(&ev, t_ms, None);
            (g.id.clone(), ev, entries)
        }) else {
            return Vec::new();
        };
        self.journal_do(|j| entries.iter().try_for_each(|e| j.log(&game_id, e)));
        ev.into_iter().map(ev_out).collect()
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
        ev.into_iter().map(ev_out).collect()
    }

    /// Record locations the server already has as checked.
    pub fn mark_checked(&self, ids: Vec<i64>, now_ms: i64) {
        let dir = self.dir.clone();
        self.with_game(|g| {
            g.mark_checked(&ids, now_ms);
            let _ = g.save(&dir);
        });
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
        self.with_game(|g| g.set_counting(on, t_ms));
    }

    /// When the next time-away mark falls due if nothing changes: the app schedules one wake-up then and calls [`Self::tick`].
    pub fn next_due_ms(&self, now_ms: i64) -> Option<i64> {
        self.with_game(|g| g.next_due_ms(now_ms)).flatten()
    }

    /// A scheduled wake-up: credit time away up to `t_ms` and report what completed.
    pub fn tick(&self, t_ms: i64) -> Vec<EventOut> {
        let Some((id, entries, ev)) = self.with_game(|g| {
            let ev = g.tick(t_ms);
            self.save_if_due(g, t_ms, !ev.is_empty());
            (g.id.clone(), g.journal_events(&ev, t_ms, None), ev)
        }) else {
            return Vec::new();
        };
        self.journal_do(|j| entries.iter().try_for_each(|e| j.log(&id, e)));
        ev.into_iter().map(ev_out).collect()
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

    /// Everything that happened in the open game between two moments.
    pub fn away_report(&self, from_ms: i64, to_ms: i64) -> Option<AwayReportOut> {
        let id = self.game_id()?;
        let j = self.journal.as_ref()?.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let s = j.summary(&id, from_ms, to_ms).ok()?;
        let events = j.events_since(&id, from_ms).ok()?;
        drop(j);
        Some(AwayReportOut {
            from_ms,
            to_ms,
            points: s.points,
            simulated_points: s.simulated_points,
            distance_m: s.distance_m,
            counts: s.by_kind.into_iter().map(|(kind, count)| KindCountOut { kind, count }).collect(),
            events: events
                .into_iter()
                .filter(|e| e.t_ms <= to_ms)
                .map(|e| AuditEventOut { t_ms: e.t_ms, kind: e.kind, detail: e.detail, at: e.at.map(|(lat, lon)| GeoPoint { lat, lon }) })
                .collect(),
        })
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
    use apgo_core::scan::build_atlas;

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
            zone_modes: vec![Mode::Walk],
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
            },
            &e.catalog,
        )
        .unwrap();
        e.install(game);
        e
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
    fn editing_another_realm_leaves_the_zones_alone() {
        let e = engine_with_game("other-edit");
        e.save_realm("r1".into(), "R1".into(), None, Some(circle(destination(home(), 0.0, 50_000.0), 2000.0)), vec![], false).unwrap();
        let p = home();
        e.on_fix(fix_at(p, FIX_T_MS + 1_000), None, false);
        assert_eq!(e.last_zone_proximity(), "inside");
    }

    /// A fix time in the core's sane range (after 2000-01-01, adversarial review M1).
    const FIX_T_MS: i64 = 1_800_000_005_000;

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
            zone_modes: vec![Mode::Walk],
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
        let older = Game::build_streets(&e.zone_atlases(&zones).iter().collect::<Vec<_>>());
        atlas.ways = apgo_core::loc::bench::grid_ways(corner, 6, 100.0);
        e.store().save_atlas(&atlas).unwrap(); // a rescan in between
        let newer = Game::build_streets(&e.zone_atlases(&zones).iter().collect::<Vec<_>>());
        assert!(e.swap_streets(&id, &zones, newer, newer_gen));
        assert!(!e.swap_streets(&id, &zones, older, older_gen), "the older build finished last but is not swapped in");
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
    fn the_travel_mode_comes_from_the_last_estimate_and_from_the_raw_fix_only_before_the_first() {
        // Ruling E2: a Walk zone here, a Bike zone 10 km north.
        let walk = Point::new(40.0, -111.0);
        let bike = destination(walk, 0.0, 10_000.0);
        let zones = [(Shape::Circle { center: walk, radius_m: 500.0 }, Mode::Walk), (Shape::Circle { center: bike, radius_m: 500.0 }, Mode::Bike)];
        assert_eq!(fix_mode(&zones, None, walk), Some(Mode::Walk), "the first fix: its raw position");
        assert_eq!(fix_mode(&zones, Some(bike), walk), Some(Mode::Bike), "later: the last estimate, never the raw fix");
        assert_eq!(fix_mode(&zones, Some(destination(walk, 0.0, 5_000.0)), walk), Some(Mode::Bike), "between zones: fastest");
        assert_eq!(fix_mode(&[], None, walk), None);
    }

    #[test]
    fn a_forager_reports_its_items_and_counts_and_other_quests_report_none() {
        let (a, b) = (Point::new(40.0, -111.0), Point::new(40.01, -111.0));
        let t = Target::Collect { pts: vec![a, b], need: 1, r: 25.0, theme: "shells".into() };
        let c = Collected { picked: BTreeSet::from([0]), carried: 1, banked: 0 };
        let out = collect_out(&t, Some(&c)).expect("a forager has items");
        assert_eq!((out.theme.as_str(), out.need, out.carried, out.banked), ("shells", 1, 1, 0));
        assert_eq!(out.items.iter().map(|i| i.picked).collect::<Vec<_>>(), [true, false]);
        assert!((out.items[1].at.lat - 40.01).abs() < 1e-12);
        assert_eq!(collect_out(&t, None).unwrap().items.iter().filter(|i| i.picked).count(), 0, "nothing picked yet");
        assert!(collect_out(&Target::Point { p: a, r: 40.0 }, None).is_none());
        assert_eq!(first_open(&out).map(|p| p.lat), Some(40.01), "the pin to open is the first item still out there");
    }

    #[test]
    fn a_done_forager_keeps_its_anchor_and_an_unfinished_one_sits_on_its_first_open_item() {
        let (a, b) = (Point::new(40.0, -111.0), Point::new(40.01, -111.0));
        let t = Target::Collect { pts: vec![a, b], need: 1, r: 25.0, theme: "shells".into() };
        let c = collect_out(&t, Some(&Collected { picked: BTreeSet::from([0]), carried: 0, banked: 1 }));
        let lat = |s| pin_at(c.as_ref(), s, Some(a)).map(|p| p.lat);
        assert_eq!(lat(QuestState::Done), Some(40.0), "a done forager stays in the places list like any done quest");
        assert_eq!(lat(QuestState::InProgress), Some(40.01));
        assert_eq!(pin_at(None, QuestState::Done, Some(b)).map(|p| p.lat), Some(40.01), "other quests keep their anchor");
    }
}
