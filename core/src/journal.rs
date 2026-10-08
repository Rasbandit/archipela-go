//! On-device journal in SQLite: every accepted GPS fix (the trace) and an audit log of what happened while you were out.
//! One file for all games; rows carry the game id. Points also feed an R*Tree so map-viewport queries stay fast.

use std::collections::BTreeMap;
use std::path::Path;

use rusqlite::Connection;

use crate::game::Event;
use crate::geo::{distance_m, Point};

/// One recorded GPS fix.
#[derive(Debug, Clone, PartialEq)]
pub struct TrackPoint {
    /// When the fix was taken, in Unix milliseconds.
    pub t_ms: i64,
    /// Latitude in degrees.
    pub lat: f64,
    /// Longitude in degrees.
    pub lon: f64,
    /// Horizontal accuracy in metres.
    pub accuracy_m: f64,
    /// Dev simulator position, not real GPS.
    pub simulated: bool,
}

/// One line of the audit log.
#[derive(Debug, Clone, PartialEq)]
pub struct JournalEvent {
    /// When it happened, in Unix milliseconds.
    pub t_ms: i64,
    /// Short machine name, e.g. `quest_done`, `fix_rejected`, `app_background` (see the constants below).
    pub kind: String,
    /// Human-readable detail of the event.
    pub detail: String,
    /// Where the player was, as (latitude, longitude), when known.
    pub at: Option<(f64, f64)>,
}

impl JournalEvent {
    /// The log entry for a game event, stamped with `t_ms` and an optional position.
    #[must_use]
    pub fn from_game_event(e: &Event, t_ms: i64, at: Option<(f64, f64)>) -> Self {
        let (kind, detail) = match e {
            Event::QuestDone { name, .. } => (kind::QUEST_DONE, name.clone()),
            Event::SendCheck { location_id } => (kind::CHECK_SENT, location_id.to_string()),
            Event::Reward { item, .. } => (kind::REWARD, item.clone()),
            Event::ZoneUnlocked { zone } => (kind::ZONE_UNLOCKED, zone.to_string()),
            Event::Trap { item, message } => (kind::TRAP, format!("{item}: {message}")),
            Event::ShuffleRequested => (kind::INFO, "Shuffle requested".to_string()),
            Event::Discovered { location_id } => (kind::DISCOVERED, location_id.to_string()),
            Event::GoalAchieved { label } => (kind::GOAL, label.clone()),
            Event::Info { text } => (kind::INFO, text.clone()),
        };
        Self { t_ms, kind: kind.to_string(), detail, at }
    }
}

/// Names for the `kind` of a journal event.
pub mod kind {
    /// A quest was completed.
    pub const QUEST_DONE: &str = "quest_done";
    /// A check was sent to the server.
    pub const CHECK_SENT: &str = "check_sent";
    /// A reward was received.
    pub const REWARD: &str = "reward";
    /// A zone was unlocked.
    pub const ZONE_UNLOCKED: &str = "zone_unlocked";
    /// A trap started.
    pub const TRAP: &str = "trap";
    /// A quest was discovered under fog.
    pub const DISCOVERED: &str = "discovered";
    /// A goal was achieved.
    pub const GOAL: &str = "goal";
    /// General information.
    pub const INFO: &str = "info";
    /// An item arrived from the multiworld.
    pub const ITEM_RECEIVED: &str = "item_received";
    /// A GPS fix was ignored as unreliable.
    pub const FIX_REJECTED: &str = "fix_rejected";
    /// The app came to the foreground.
    pub const APP_FOREGROUND: &str = "app_foreground";
    /// The app went to the background.
    pub const APP_BACKGROUND: &str = "app_background";
    /// The player paused or resumed play (tracking off or on).
    pub const PLAY_PAUSED: &str = "play_paused";
    /// Play resumed after a pause.
    pub const PLAY_RESUMED: &str = "play_resumed";
    /// A presence rule (home Wi-Fi, car) paused or resumed counting.
    pub const PRESENCE: &str = "presence";
    /// Close to a quest that did not count; the detail says why.
    pub const NEAR_MISS: &str = "near_miss";
}

/// What happened between two moments: the "while you were out" report.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Summary {
    /// Start of the period, in Unix milliseconds.
    pub from_ms: i64,
    /// End of the period, in Unix milliseconds.
    pub to_ms: i64,
    /// Number of GPS points recorded.
    pub points: u32,
    /// How many of those points came from the dev simulator.
    pub simulated_points: u32,
    /// Distance travelled, in metres.
    pub distance_m: f64,
    /// Number of events by kind.
    pub by_kind: BTreeMap<String, u32>,
}

/// The on-device SQLite journal of GPS fixes and events.
pub struct Journal {
    conn: Connection,
}

/// Gaps longer than this split the trace into separate line segments (phone off, app closed).
pub const DEFAULT_MAX_GAP_MS: i64 = 2 * 60_000;

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS points (
    id INTEGER PRIMARY KEY,
    game TEXT NOT NULL,
    t_ms INTEGER NOT NULL,
    lat REAL NOT NULL,
    lon REAL NOT NULL,
    accuracy_m REAL NOT NULL,
    simulated INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS points_game_t ON points (game, t_ms);
CREATE VIRTUAL TABLE IF NOT EXISTS points_rt USING rtree (id, min_lat, max_lat, min_lon, max_lon);
CREATE TABLE IF NOT EXISTS events (
    id INTEGER PRIMARY KEY,
    game TEXT NOT NULL,
    t_ms INTEGER NOT NULL,
    kind TEXT NOT NULL,
    detail TEXT NOT NULL,
    lat REAL,
    lon REAL
);
CREATE INDEX IF NOT EXISTS events_game_t ON events (game, t_ms);
";

const POINT_COLS: &str = "t_ms, lat, lon, accuracy_m, simulated";

fn point_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<TrackPoint> {
    Ok(TrackPoint { t_ms: r.get(0)?, lat: r.get(1)?, lon: r.get(2)?, accuracy_m: r.get(3)?, simulated: r.get::<_, i64>(4)? != 0 })
}

impl Journal {
    fn init(conn: Connection) -> rusqlite::Result<Self> {
        conn.execute_batch(SCHEMA)?;
        Ok(Self { conn })
    }

    /// Open (or create) the journal database at `path`.
    ///
    /// # Errors
    /// Returns an error if the file cannot be opened or the schema cannot be created.
    pub fn open(path: &Path) -> rusqlite::Result<Self> {
        let conn = Connection::open(path)?;
        // Write-ahead log: a fix every few seconds must not block, and a crash must not lose the trace.
        conn.pragma_update(None, "journal_mode", "WAL")?;
        Self::init(conn)
    }

    /// An in-memory journal, for tests and previews.
    ///
    /// # Errors
    /// Returns an error if the schema cannot be created.
    pub fn open_memory() -> rusqlite::Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    /// Record an accepted GPS fix for `game`.
    ///
    /// # Errors
    /// Returns any SQLite error.
    pub fn add_point(&self, game: &str, p: &TrackPoint) -> rusqlite::Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "INSERT INTO points (game, t_ms, lat, lon, accuracy_m, simulated) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            (game, p.t_ms, p.lat, p.lon, p.accuracy_m, i64::from(p.simulated)),
        )?;
        let id = tx.last_insert_rowid();
        tx.execute("INSERT INTO points_rt (id, min_lat, max_lat, min_lon, max_lon) VALUES (?1, ?2, ?2, ?3, ?3)", (id, p.lat, p.lon))?;
        tx.commit()
    }

    /// Points of a game in time order, `from_ms..=to_ms`.
    ///
    /// # Errors
    /// Returns any SQLite error.
    pub fn track(&self, game: &str, from_ms: i64, to_ms: i64) -> rusqlite::Result<Vec<TrackPoint>> {
        let sql = format!("SELECT {POINT_COLS} FROM points WHERE game = ?1 AND t_ms BETWEEN ?2 AND ?3 ORDER BY t_ms, id");
        self.conn.prepare(&sql)?.query_map((game, from_ms, to_ms), point_row)?.collect()
    }

    /// The trace as separate lines, split wherever two points are further apart in time than `max_gap_ms`.
    ///
    /// # Errors
    /// Returns any SQLite error.
    pub fn segments(&self, game: &str, from_ms: i64, to_ms: i64, max_gap_ms: i64) -> rusqlite::Result<Vec<Vec<TrackPoint>>> {
        let mut out: Vec<Vec<TrackPoint>> = Vec::new();
        for p in self.track(game, from_ms, to_ms)? {
            match out.last_mut() {
                Some(seg) if seg.last().is_some_and(|l| p.t_ms - l.t_ms <= max_gap_ms) => seg.push(p),
                _ => out.push(vec![p]),
            }
        }
        Ok(out)
    }

    /// Points inside a lat/lon box (map viewport), time-ordered.
    ///
    /// # Errors
    /// Returns any SQLite error.
    pub fn points_in_box(&self, game: &str, min: (f64, f64), max: (f64, f64)) -> rusqlite::Result<Vec<TrackPoint>> {
        let sql = format!(
            "SELECT {cols} FROM points p JOIN points_rt r ON r.id = p.id
             WHERE p.game = ?1 AND r.min_lat >= ?2 AND r.max_lat <= ?3 AND r.min_lon >= ?4 AND r.max_lon <= ?5
             ORDER BY p.t_ms, p.id",
            cols = POINT_COLS.split(", ").map(|c| format!("p.{c}")).collect::<Vec<_>>().join(", ")
        );
        self.conn.prepare(&sql)?.query_map((game, min.0, max.0, min.1, max.1), point_row)?.collect()
    }

    /// Append an event to the audit log of `game`.
    ///
    /// # Errors
    /// Returns any SQLite error.
    pub fn log(&self, game: &str, e: &JournalEvent) -> rusqlite::Result<()> {
        let (lat, lon) = e.at.unzip();
        self.conn
            .execute("INSERT INTO events (game, t_ms, kind, detail, lat, lon) VALUES (?1, ?2, ?3, ?4, ?5, ?6)", (game, e.t_ms, &e.kind, &e.detail, lat, lon))?;
        Ok(())
    }

    /// Events of a game with `t_ms >= since_ms`, oldest first.
    ///
    /// # Errors
    /// Returns any SQLite error.
    pub fn events_since(&self, game: &str, since_ms: i64) -> rusqlite::Result<Vec<JournalEvent>> {
        self.events_between(game, since_ms, i64::MAX)
    }

    fn events_between(&self, game: &str, from_ms: i64, to_ms: i64) -> rusqlite::Result<Vec<JournalEvent>> {
        self.conn
            .prepare("SELECT t_ms, kind, detail, lat, lon FROM events WHERE game = ?1 AND t_ms BETWEEN ?2 AND ?3 ORDER BY t_ms, id")?
            .query_map((game, from_ms, to_ms), Self::event_row)?
            .collect()
    }

    /// The newest `limit` events of a game, newest first (the activity view).
    ///
    /// # Errors
    /// Returns any SQLite error.
    pub fn recent_events(&self, game: &str, limit: u32) -> rusqlite::Result<Vec<JournalEvent>> {
        self.conn
            .prepare("SELECT t_ms, kind, detail, lat, lon FROM events WHERE game = ?1 ORDER BY t_ms DESC, id DESC LIMIT ?2")?
            .query_map((game, limit), Self::event_row)?
            .collect()
    }

    fn event_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<JournalEvent> {
        let (lat, lon): (Option<f64>, Option<f64>) = (r.get(3)?, r.get(4)?);
        Ok(JournalEvent { t_ms: r.get(0)?, kind: r.get(1)?, detail: r.get(2)?, at: lat.zip(lon) })
    }

    /// Time of the newest event of this kind for a game.
    ///
    /// # Errors
    /// Returns any SQLite error.
    pub fn last_of_kind(&self, game: &str, kind: &str) -> rusqlite::Result<Option<i64>> {
        self.conn.query_row("SELECT MAX(t_ms) FROM events WHERE game = ?1 AND kind = ?2", (game, kind), |r| r.get(0))
    }

    /// Count points, distance and events of `game` between `from_ms` and `to_ms`.
    ///
    /// # Errors
    /// Returns any SQLite error.
    pub fn summary(&self, game: &str, from_ms: i64, to_ms: i64) -> rusqlite::Result<Summary> {
        let mut s = Summary { from_ms, to_ms, ..Summary::default() };
        let mut prev: Option<TrackPoint> = None;
        for p in self.track(game, from_ms, to_ms)? {
            s.points += 1;
            s.simulated_points += u32::from(p.simulated);
            // Same rule as the live distance counter: a long gap is not walked.
            if let Some(l) = prev.as_ref().filter(|l| p.t_ms - l.t_ms <= DEFAULT_MAX_GAP_MS) {
                s.distance_m += distance_m(Point::new(l.lat, l.lon), Point::new(p.lat, p.lon));
            }
            prev = Some(p);
        }
        for e in self.events_between(game, from_ms, to_ms)? {
            *s.by_kind.entry(e.kind).or_default() += 1;
        }
        Ok(s)
    }

    /// Delete all points and events of `game`.
    ///
    /// # Errors
    /// Returns any SQLite error.
    pub fn clear_game(&self, game: &str) -> rusqlite::Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        tx.execute("DELETE FROM points_rt WHERE id IN (SELECT id FROM points WHERE game = ?1)", [game])?;
        tx.execute("DELETE FROM points WHERE game = ?1", [game])?;
        tx.execute("DELETE FROM events WHERE game = ?1", [game])?;
        tx.commit()
    }
}

#[cfg(test)]
#[allow(clippy::assert_is_empty)] // test code: `is_empty()` reads better in assertions than comparing with a typed empty array
mod tests {
    use super::*;

    fn pt(t_ms: i64, lat: f64, lon: f64) -> TrackPoint {
        TrackPoint { t_ms, lat, lon, accuracy_m: 5.0, simulated: false }
    }
    fn ev(t_ms: i64, k: &str) -> JournalEvent {
        JournalEvent { t_ms, kind: k.into(), detail: format!("{k} detail"), at: Some((40.0, -111.0)) }
    }

    #[test]
    fn track_is_time_ordered_and_scoped_to_game_and_range() {
        let j = Journal::open_memory().unwrap();
        j.add_point("g1", &pt(3000, 40.003, -111.0)).unwrap();
        j.add_point("g1", &pt(1000, 40.001, -111.0)).unwrap();
        j.add_point("g1", &pt(2000, 40.002, -111.0)).unwrap();
        j.add_point("g2", &pt(1500, 41.0, -111.0)).unwrap();
        let all = j.track("g1", 0, i64::MAX).unwrap();
        assert_eq!(all.iter().map(|p| p.t_ms).collect::<Vec<_>>(), vec![1000, 2000, 3000]);
        let mid = j.track("g1", 1500, 2500).unwrap();
        assert_eq!(mid.len(), 1);
        assert_eq!(mid[0].t_ms, 2000);
        assert!(j.track("nope", 0, i64::MAX).unwrap().is_empty());
    }

    #[test]
    fn point_keeps_all_fields_including_simulated_flag() {
        let j = Journal::open_memory().unwrap();
        let p = TrackPoint { t_ms: 7, lat: 40.5, lon: -111.5, accuracy_m: 12.5, simulated: true };
        j.add_point("g", &p).unwrap();
        assert_eq!(j.track("g", 0, 100).unwrap(), vec![p]);
    }

    #[test]
    fn segments_split_on_long_gaps() {
        let j = Journal::open_memory().unwrap();
        for (t, lat) in [(0, 40.0), (10_000, 40.001), (20_000, 40.002), (1_000_000, 40.5), (1_010_000, 40.501)] {
            j.add_point("g", &pt(t, lat, -111.0)).unwrap();
        }
        let segs = j.segments("g", 0, i64::MAX, DEFAULT_MAX_GAP_MS).unwrap();
        assert_eq!(segs.iter().map(Vec::len).collect::<Vec<_>>(), vec![3, 2]);
        assert!(j.segments("g", 0, i64::MAX, 0).unwrap().len() >= 5, "zero gap isolates every point");
        assert!(j.segments("empty", 0, i64::MAX, DEFAULT_MAX_GAP_MS).unwrap().is_empty());
    }

    #[test]
    fn box_query_uses_game_and_bounds() {
        let j = Journal::open_memory().unwrap();
        j.add_point("g", &pt(1, 40.0, -111.0)).unwrap();
        j.add_point("g", &pt(2, 40.5, -111.0)).unwrap();
        j.add_point("other", &pt(3, 40.0, -111.0)).unwrap();
        let hit = j.points_in_box("g", (39.9, -111.1), (40.1, -110.9)).unwrap();
        assert_eq!(hit.iter().map(|p| p.t_ms).collect::<Vec<_>>(), vec![1]);
    }

    #[test]
    fn events_since_are_ordered_scoped_and_roundtrip() {
        let j = Journal::open_memory().unwrap();
        j.log("g", &ev(300, kind::QUEST_DONE)).unwrap();
        j.log("g", &ev(100, kind::APP_BACKGROUND)).unwrap();
        j.log("g", &JournalEvent { t_ms: 200, kind: kind::INFO.into(), detail: "x".into(), at: None }).unwrap();
        j.log("other", &ev(250, kind::TRAP)).unwrap();
        let got = j.events_since("g", 150).unwrap();
        assert_eq!(got.iter().map(|e| e.t_ms).collect::<Vec<_>>(), vec![200, 300]);
        assert_eq!(got[0].at, None);
        assert_eq!(got[1], ev(300, kind::QUEST_DONE));
    }

    #[test]
    fn summary_counts_points_distance_and_kinds() {
        let j = Journal::open_memory().unwrap();
        // ~111 m north per 0.001 degree latitude.
        j.add_point("g", &pt(0, 40.000, -111.0)).unwrap();
        j.add_point("g", &pt(10_000, 40.001, -111.0)).unwrap();
        j.add_point("g", &TrackPoint { simulated: true, ..pt(20_000, 40.002, -111.0) }).unwrap();
        j.log("g", &ev(5, kind::QUEST_DONE)).unwrap();
        j.log("g", &ev(6, kind::QUEST_DONE)).unwrap();
        j.log("g", &ev(7, kind::FIX_REJECTED)).unwrap();
        j.log("g", &ev(99_999, kind::TRAP)).unwrap(); // out of range
        let s = j.summary("g", 0, 30_000).unwrap();
        assert_eq!((s.points, s.simulated_points), (3, 1));
        assert!((s.distance_m - 222.0).abs() < 5.0, "got {}", s.distance_m);
        assert_eq!(s.by_kind.get(kind::QUEST_DONE), Some(&2));
        assert_eq!(s.by_kind.get(kind::FIX_REJECTED), Some(&1));
        assert_eq!(s.by_kind.get(kind::TRAP), None);
    }

    #[test]
    fn summary_ignores_jumps_across_gaps_for_distance() {
        let j = Journal::open_memory().unwrap();
        j.add_point("g", &pt(0, 40.0, -111.0)).unwrap();
        j.add_point("g", &pt(10_000_000, 41.0, -111.0)).unwrap(); // hours later, 111 km away: not walked
        assert!(j.summary("g", 0, i64::MAX).unwrap().distance_m < 1.0);
    }

    #[test]
    fn game_events_map_to_audit_kinds() {
        use crate::game::Event;
        let at = Some((40.0, -111.0));
        let cases = [
            (Event::QuestDone { location_id: 1, name: "Easy Walk Quest #1".into() }, kind::QUEST_DONE, "Easy Walk Quest #1"),
            (Event::SendCheck { location_id: 7 }, kind::CHECK_SENT, "7"),
            (Event::Reward { location_id: 1, item: "Bike".into() }, kind::REWARD, "Bike"),
            (Event::ZoneUnlocked { zone: 2 }, kind::ZONE_UNLOCKED, "2"),
            (Event::Trap { item: "Freeze".into(), message: "Frozen".into() }, kind::TRAP, "Freeze: Frozen"),
            (Event::Discovered { location_id: 3 }, kind::DISCOVERED, "3"),
            (Event::GoalAchieved { label: "Win".into() }, kind::GOAL, "Win"),
            (Event::Info { text: "hi".into() }, kind::INFO, "hi"),
            (Event::ShuffleRequested, kind::INFO, "Shuffle requested"),
        ];
        for (e, k, d) in cases {
            let got = JournalEvent::from_game_event(&e, 42, at);
            assert_eq!((got.kind.as_str(), got.detail.as_str(), got.t_ms, got.at), (k, d, 42, at), "{e:?}");
        }
    }

    #[test]
    fn recent_events_are_newest_first_limited_and_scoped() {
        let j = Journal::open_memory().unwrap();
        for t in [100, 300, 200, 400] {
            j.log("g", &ev(t, kind::INFO)).unwrap();
        }
        j.log("other", &ev(999, kind::INFO)).unwrap();
        let got = j.recent_events("g", 3).unwrap();
        assert_eq!(got.iter().map(|e| e.t_ms).collect::<Vec<_>>(), vec![400, 300, 200]);
        assert!(j.recent_events("g", 0).unwrap().is_empty());
        assert!(j.recent_events("nope", 10).unwrap().is_empty());
    }

    #[test]
    fn last_of_kind_is_the_latest_time_for_that_game() {
        let j = Journal::open_memory().unwrap();
        j.log("g", &ev(100, kind::APP_BACKGROUND)).unwrap();
        j.log("g", &ev(500, kind::APP_BACKGROUND)).unwrap();
        j.log("g", &ev(900, kind::APP_FOREGROUND)).unwrap();
        j.log("other", &ev(999, kind::APP_BACKGROUND)).unwrap();
        assert_eq!(j.last_of_kind("g", kind::APP_BACKGROUND).unwrap(), Some(500));
        assert_eq!(j.last_of_kind("g", kind::TRAP).unwrap(), None);
    }

    #[test]
    fn clear_game_removes_only_that_game() {
        let j = Journal::open_memory().unwrap();
        j.add_point("a", &pt(1, 40.0, -111.0)).unwrap();
        j.add_point("b", &pt(1, 40.0, -111.0)).unwrap();
        j.log("a", &ev(1, kind::INFO)).unwrap();
        j.clear_game("a").unwrap();
        assert!(j.track("a", 0, 9).unwrap().is_empty());
        assert!(j.points_in_box("a", (0.0, -180.0), (90.0, 0.0)).unwrap().is_empty());
        assert!(j.events_since("a", 0).unwrap().is_empty());
        assert_eq!(j.track("b", 0, 9).unwrap().len(), 1);
    }

    #[test]
    fn file_database_persists_across_opens() {
        let dir = std::env::temp_dir().join(format!("apgo-journal-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("journal.db");
        let _ = std::fs::remove_file(&path);
        Journal::open(&path).unwrap().add_point("g", &pt(1, 40.0, -111.0)).unwrap();
        assert_eq!(Journal::open(&path).unwrap().track("g", 0, 9).unwrap().len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
