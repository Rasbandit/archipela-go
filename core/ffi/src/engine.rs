//! `UniFFI` facade over the game engine: realms, scanning, game setup, play. Blocking calls; call from a background thread.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};

use apgo_core::assign::SurfacePref;
use apgo_core::assign::Target;
use apgo_core::catalog::{Catalog, Kind, Mode};
use apgo_core::chain::ChainUnit;
use apgo_core::game::{Backend, Event, Game, NearMiss, NewGame, QuestState};
use apgo_core::geo::{distance_m, simplify, Point};
use apgo_core::journal::{kind, Journal, JournalEvent, TrackPoint, DEFAULT_MAX_GAP_MS};
use apgo_core::marks::Mark;
use apgo_core::num::count_u32;
use apgo_core::realm::{Proximity, Realm, RealmStore, Shape};
use apgo_core::save_policy::SavePolicy;
use apgo_core::scan::{scan_realm, Atlas};
use apgo_core::settings::{resolve_units, Settings};
use apgo_core::slot::SlotData;
use apgo_core::solo::{generate, SoloOptions};
use apgo_core::units::{distance, distance_rounded, Round, UnitSystem};
use apgo_core::verify::{Fix, MAX_ACCURACY_M};
use apgo_core::yaml::build_yaml;

use crate::{CoreError, GeoPoint};

#[allow(clippy::needless_pass_by_value)] // used as a `map_err` callback, which hands over the error by value
fn err<E: ToString>(e: E) -> CoreError {
    CoreError::Failed { detail: e.to_string() }
}

fn gp(p: Point) -> GeoPoint {
    GeoPoint { lat: p.lat, lon: p.lon }
}

fn pt(p: &GeoPoint) -> Point {
    Point::new(p.lat, p.lon)
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
    /// Shapes of the open game's zone realms, for the "inside a zone" check on each fix.
    zone_shapes: Mutex<Vec<Shape>>,
    // Where the last fix was relative to the zones, worked out once in `on_fix` for presence to read.
    last_proximity: Mutex<Option<Proximity>>,
    /// The phone's region (ISO country code) that `Auto` units follow; empty until the app sets it.
    region: Mutex<String>,
    /// The units text is written in, worked out from the unit setting and `region` whenever either changes.
    units: Mutex<UnitSystem>,
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

    /// Make `game` the open game and remember the shapes of its zones' realms (for "inside a zone" checks).
    fn install(&self, mut game: Game) {
        let store = self.store();
        if !game.streets_attached() {
            // A saved game keeps only a thin sample of streets: index every street of its zones so trap targets land on one.
            let atlases: Vec<Atlas> =
                game.zone_realms.iter().collect::<BTreeSet<_>>().into_iter().filter_map(|id| store.get(id)).filter_map(|r| self.zoned_atlas(&r)).collect();
            game.attach_streets(&atlases.iter().collect::<Vec<_>>());
        }
        self.save_policy.lock().unwrap_or_else(std::sync::PoisonError::into_inner).reset();
        // Only one game is played at a time; remember which, so closing the app without pausing resumes it on the next start.
        if let Err(e) = Game::mark_playing(&self.dir, &game.id) {
            self.note(format!("could not remember game {} as being played: {e}", game.id));
        }
        let shapes = self.shapes_of(&game.zone_realms);
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
        *slot = Some(game);
        self.set_zones(slot, shapes);
    }

    /// Realm `id` was redrawn or deleted: when it is a zone of the open game, in-zone checks use what is saved now, not what was
    /// saved when the game was opened.
    fn refresh_zones_of(&self, id: &str) {
        let slot = self.game.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let Some(shapes) = slot.as_ref().map(|g| &g.zone_realms).filter(|z| z.iter().any(|r| r == id)).map(|z| self.shapes_of(z)) else { return };
        self.set_zones(slot, shapes);
    }

    /// The current shapes of the realms `ids` (the zones of a game), as saved now.
    fn shapes_of(&self, ids: &[String]) -> Vec<Shape> {
        let store = self.store();
        ids.iter().filter_map(|id| store.get(id)).map(|r| r.shape).collect()
    }

    // The open game's zone shapes; a new set (another game, or none) forgets where the last fix was relative to the old ones.
    // Takes the game guard and releases it after, so the shapes only ever change together with the game, never between another
    // thread's read and write (lock order: game, then zones).
    fn set_zones(&self, game: MutexGuard<'_, Option<Game>>, shapes: Vec<Shape>) {
        *self.zone_shapes.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = shapes;
        *self.last_proximity.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = None;
        drop(game);
    }

    /// Distance in metres from a point to the nearest zone area of the open game (0 inside), or `None` with no game.
    fn zone_distance_m(&self, p: Point) -> Option<f64> {
        let shapes = self.zone_shapes.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        shapes.iter().map(|s| s.distance_m(p)).reduce(f64::min)
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
            zone_shapes: Mutex::new(Vec::new()),
            last_proximity: Mutex::new(None),
            units: Mutex::new(resolve_units(Settings::load(&dir).units, "")),
            region: Mutex::new(String::new()),
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
    pub fn on_fix(&self, lat: f64, lon: f64, t_ms: i64, accuracy_m: f64, steps: Option<i64>, simulated: bool) -> Vec<EventOut> {
        let fix = Fix { lat, lon, t_ms, accuracy_m };
        let at = Some((lat, lon));
        // One pass over the zone shapes per fix: the inside flag for the game and the proximity presence reads next.
        let zone_d = self.zone_distance_m(Point::new(lat, lon));
        if self.has_game() {
            *self.last_proximity.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = Proximity::of_distance(zone_d);
        }
        let Some((game_id, ev, entries, near)) = self
            .with_game(|g| {
                let ev = g.on_fix(fix, steps);
                self.save_if_due(g, t_ms, !ev.is_empty());
                (g.id.clone(), g.journal_events(&ev, t_ms, at), g.explain_near(&fix, NEAR_MISS_RADIUS_M), ev)
            })
            .map(|(id, entries, near, ev)| (id, ev, entries, near))
        else {
            return Vec::new();
        };
        self.journal_do(|j| {
            if accuracy_m > MAX_ACCURACY_M {
                let last_ms = self.last_reject_log_ms.load(std::sync::atomic::Ordering::Relaxed);
                if t_ms.saturating_sub(last_ms) >= 60_000 {
                    self.last_reject_log_ms.store(t_ms, std::sync::atomic::Ordering::Relaxed);
                    j.log(
                        &game_id,
                        &JournalEvent {
                            t_ms,
                            kind: kind::FIX_REJECTED.into(),
                            detail: format!("accuracy {}", distance_rounded(accuracy_m, self.unit_system(), Round::Up)),
                            at,
                        },
                    )?;
                }
            } else {
                j.add_point(&game_id, &TrackPoint { t_ms, lat, lon, accuracy_m, simulated })?;
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

    /// A step-counter reading from the phone (cumulative since boot). Only counts while a game is open.
    pub fn on_steps(&self, total: i64, t_ms: i64) -> Vec<EventOut> {
        let Some((game_id, ev, entries)) = self.with_game(|g| {
            let ev = g.on_steps(total, t_ms);
            self.save_if_due(g, t_ms, !ev.is_empty());
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
                    points: simplify(&pts, TRACE_MIN_STEP_M, TRACE_TOLERANCE_M).into_iter().map(|p| GeoPoint { lat: p.lat, lon: p.lon }).collect(),
                }
            })
            .collect()
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
        e.on_fix(p.lat, p.lon, 1_000, 5.0, None, false);
        assert_eq!(e.last_zone_proximity(), "inside");
        let far = destination(home(), 0.0, 50_000.0);
        e.save_realm("r0".into(), "R0".into(), None, Some(circle(far, 2000.0)), vec![], false).unwrap();
        e.on_fix(p.lat, p.lon, 2_000, 5.0, None, false);
        assert_eq!(e.last_zone_proximity(), "far");
    }

    #[test]
    fn deleting_a_zone_realm_of_the_open_game_drops_its_zone() {
        let e = engine_with_game("realm-delete");
        let p = home();
        e.on_fix(p.lat, p.lon, 1_000, 5.0, None, false);
        assert_eq!(e.last_zone_proximity(), "inside");
        e.delete_realm("r0".into()).unwrap();
        e.on_fix(p.lat, p.lon, 2_000, 5.0, None, false);
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
        e.on_fix(p.lat, p.lon, 1_000, 5.0, None, false);
        assert_eq!(e.last_zone_proximity(), "inside");
    }
}
