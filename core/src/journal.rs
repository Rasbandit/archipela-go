//! On-device journal in SQLite: every accepted GPS fix (the trace) and an audit log of what happened while you were out.
//! One file for all games; rows carry the game id. A fix's point and its events are written in one transaction (`Journal::batch`).

use std::path::Path;

use rusqlite::Connection;

use crate::game::Event;
use crate::recap::Recap;

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
            Event::Trap { item, message, .. } => (kind::TRAP, format!("{item}: {message}")),
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
    /// A walk closed into a recap; the detail is its one-line summary.
    pub const WALK: &str = "walk";
}

/// The on-device SQLite journal of GPS fixes and events.
pub struct Journal {
    conn: Connection,
    // Bumped on every logged event, so the app reloads the activity log only when it changed (instead of on a timer).
    revision: std::cell::Cell<u64>,
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
CREATE TABLE IF NOT EXISTS walks (
    id INTEGER PRIMARY KEY,
    game TEXT NOT NULL,
    end_ms INTEGER NOT NULL,
    recap TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS walks_game_end ON walks (game, end_ms);
";

// Journals before 2026-10 mirrored every point into an R*Tree that only a test read: drop it (the points stay in `points`).
const DROP_OLD: &str = "DROP TABLE IF EXISTS points_rt;";

const POINT_COLS: &str = "t_ms, lat, lon, accuracy_m, simulated";

fn point_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<TrackPoint> {
    Ok(TrackPoint { t_ms: r.get(0)?, lat: r.get(1)?, lon: r.get(2)?, accuracy_m: r.get(3)?, simulated: r.get::<_, i64>(4)? != 0 })
}

impl Journal {
    fn init(conn: Connection) -> rusqlite::Result<Self> {
        conn.execute_batch(SCHEMA)?;
        conn.execute_batch(DROP_OLD)?;
        Ok(Self { conn, revision: std::cell::Cell::new(0) })
    }

    /// Open (or create) the journal database at `path`.
    ///
    /// # Errors
    /// Returns an error if the file cannot be opened or the schema cannot be created.
    pub fn open(path: &Path) -> rusqlite::Result<Self> {
        let conn = Connection::open(path)?;
        // Write-ahead log: a fix every few seconds must not block, and a crash must not lose the trace.
        conn.pragma_update(None, "journal_mode", "WAL")?;
        // NORMAL is safe with WAL (a power cut may lose the last commits, never corrupt) and syncs at checkpoints, not on every commit.
        conn.pragma_update(None, "synchronous", "NORMAL")?;
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
        self.conn.prepare_cached("INSERT INTO points (game, t_ms, lat, lon, accuracy_m, simulated) VALUES (?1, ?2, ?3, ?4, ?5, ?6)")?.execute((
            game,
            p.t_ms,
            p.lat,
            p.lon,
            p.accuracy_m,
            i64::from(p.simulated),
        ))?;
        Ok(())
    }

    /// Run `f` as one transaction: everything it writes lands together (one commit, one WAL sync) or not at all.
    ///
    /// # Errors
    /// Returns the first error of `f` (after rolling back) or any SQLite error.
    pub fn batch<T>(&self, f: impl FnOnce(&Self) -> rusqlite::Result<T>) -> rusqlite::Result<T> {
        let tx = self.conn.unchecked_transaction()?;
        let out = f(self)?;
        tx.commit()?;
        Ok(out)
    }

    /// Points of a game in time order, `from_ms..=to_ms`.
    ///
    /// # Errors
    /// Returns any SQLite error.
    pub fn track(&self, game: &str, from_ms: i64, to_ms: i64) -> rusqlite::Result<Vec<TrackPoint>> {
        let sql = format!("SELECT {POINT_COLS} FROM points WHERE game = ?1 AND t_ms BETWEEN ?2 AND ?3 ORDER BY t_ms, id");
        self.conn.prepare_cached(&sql)?.query_map((game, from_ms, to_ms), point_row)?.collect()
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

    /// Append an event to the audit log of `game`.
    ///
    /// # Errors
    /// Returns any SQLite error.
    pub fn log(&self, game: &str, e: &JournalEvent) -> rusqlite::Result<()> {
        let (lat, lon) = e.at.unzip();
        self.conn
            .prepare_cached("INSERT INTO events (game, t_ms, kind, detail, lat, lon) VALUES (?1, ?2, ?3, ?4, ?5, ?6)")?
            .execute((game, e.t_ms, &e.kind, &e.detail, lat, lon))?;
        self.revision.set(self.revision.get() + 1);
        Ok(())
    }

    /// A number that changes whenever an event is logged (points and reads leave it alone).
    #[must_use]
    pub fn revision(&self) -> u64 {
        self.revision.get()
    }

    /// The newest `limit` events of a game, newest first (the activity view).
    ///
    /// # Errors
    /// Returns any SQLite error.
    pub fn recent_events(&self, game: &str, limit: u32) -> rusqlite::Result<Vec<JournalEvent>> {
        self.conn
            .prepare_cached("SELECT t_ms, kind, detail, lat, lon FROM events WHERE game = ?1 ORDER BY t_ms DESC, id DESC LIMIT ?2")?
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

    /// Keep the recap of a finished walk, so it can be opened again from the activity log.
    ///
    /// # Errors
    /// Returns any SQLite error, or a conversion error if the recap cannot be written as JSON.
    pub fn add_walk(&self, game: &str, r: &Recap) -> rusqlite::Result<()> {
        let json = serde_json::to_string(r).map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
        self.conn.prepare_cached("INSERT INTO walks (game, end_ms, recap) VALUES (?1, ?2, ?3)")?.execute((game, r.end_ms, json))?;
        Ok(())
    }

    /// The kept recap of the walk that ended at `end_ms` (the newest if there are several); `None` when there is none.
    ///
    /// # Errors
    /// Returns any SQLite error, or a conversion error when the stored recap cannot be read back.
    pub fn walk(&self, game: &str, end_ms: i64) -> rusqlite::Result<Option<Recap>> {
        let mut stmt = self.conn.prepare_cached("SELECT recap FROM walks WHERE game = ?1 AND end_ms = ?2 ORDER BY id DESC LIMIT 1")?;
        let mut rows = stmt.query((game, end_ms))?;
        let Some(row) = rows.next()? else { return Ok(None) };
        let json: String = row.get(0)?;
        serde_json::from_str(&json).map(Some).map_err(|e| rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e)))
    }

    /// Delete all points, events and walks of `game`.
    ///
    /// # Errors
    /// Returns any SQLite error.
    pub fn clear_game(&self, game: &str) -> rusqlite::Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        tx.execute("DELETE FROM points WHERE game = ?1", [game])?;
        tx.execute("DELETE FROM events WHERE game = ?1", [game])?;
        tx.execute("DELETE FROM walks WHERE game = ?1", [game])?;
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
    fn the_revision_moves_on_every_logged_event_and_only_then() {
        let j = Journal::open_memory().unwrap();
        let r0 = j.revision();
        j.add_point("g", &pt(1, 40.0, -111.0)).unwrap();
        let _ = j.recent_events("g", 10).unwrap();
        assert_eq!(j.revision(), r0, "points and reads leave the activity log as it was");
        j.log("g", &ev(2, "quest_done")).unwrap();
        j.log("g", &ev(3, "reward")).unwrap();
        assert_eq!(j.revision(), r0 + 2);
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
    fn logged_events_roundtrip_with_and_without_a_place() {
        let j = Journal::open_memory().unwrap();
        j.log("g", &ev(300, kind::QUEST_DONE)).unwrap();
        j.log("g", &ev(100, kind::APP_BACKGROUND)).unwrap();
        j.log("g", &JournalEvent { t_ms: 200, kind: kind::INFO.into(), detail: "x".into(), at: None }).unwrap();
        j.log("other", &ev(250, kind::TRAP)).unwrap();
        let got = j.recent_events("g", 2).unwrap();
        assert_eq!(got.iter().map(|e| e.t_ms).collect::<Vec<_>>(), vec![300, 200]);
        assert_eq!(got[1].at, None);
        assert_eq!(got[0], ev(300, kind::QUEST_DONE));
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
            (Event::Trap { item: "Freeze".into(), message: "Frozen".into(), alert: true }, kind::TRAP, "Freeze: Frozen"),
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
        assert!(j.recent_events("a", 9).unwrap().is_empty());
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

    fn temp_db(tag: &str) -> (std::path::PathBuf, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("apgo-journal-{tag}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("journal.db");
        let _ = std::fs::remove_file(&path);
        (dir, path)
    }

    #[test]
    fn file_database_uses_wal_with_normal_sync() {
        let (dir, path) = temp_db("sync");
        let j = Journal::open(&path).unwrap();
        let mode: String = j.conn.query_row("PRAGMA journal_mode", [], |r| r.get(0)).unwrap();
        let sync: i64 = j.conn.query_row("PRAGMA synchronous", [], |r| r.get(0)).unwrap();
        assert_eq!((mode.as_str(), sync), ("wal", 1), "WAL with synchronous=NORMAL: one fsync per checkpoint, not per fix");
        drop(j);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_batch_writes_all_or_nothing() {
        let j = Journal::open_memory().unwrap();
        j.batch(|j| {
            j.add_point("g", &pt(1, 40.0, -111.0))?;
            j.log("g", &ev(1, kind::INFO))
        })
        .unwrap();
        assert_eq!((j.track("g", 0, 9).unwrap().len(), j.recent_events("g", 9).unwrap().len()), (1, 1));
        let failed: rusqlite::Result<()> = j.batch(|j| {
            j.add_point("g", &pt(2, 40.0, -111.0))?;
            j.log("g", &ev(2, kind::INFO))?;
            Err(rusqlite::Error::InvalidQuery)
        });
        assert!(failed.is_err());
        assert_eq!((j.track("g", 0, 9).unwrap().len(), j.recent_events("g", 9).unwrap().len()), (1, 1), "the failed batch left nothing behind");
    }

    #[test]
    fn points_skip_the_spatial_index_and_old_databases_drop_it() {
        let (dir, path) = temp_db("rtree");
        {
            // A journal from before: points mirrored into an R*Tree.
            let c = Connection::open(&path).unwrap();
            c.execute_batch(SCHEMA).unwrap();
            c.execute_batch(
                "CREATE VIRTUAL TABLE IF NOT EXISTS points_rt USING rtree (id, min_lat, max_lat, min_lon, max_lon); INSERT INTO points_rt VALUES (1, 40, 40, -111, -111);",
            )
            .unwrap();
            c.execute("INSERT INTO points (game, t_ms, lat, lon, accuracy_m, simulated) VALUES ('g', 1, 40.0, -111.0, 5.0, 0)", []).unwrap();
        }
        let j = Journal::open(&path).unwrap();
        let rt: i64 = j.conn.query_row("SELECT COUNT(*) FROM sqlite_master WHERE name LIKE 'points_rt%'", [], |r| r.get(0)).unwrap();
        assert_eq!(rt, 0, "the R*Tree is gone");
        j.add_point("g", &pt(2, 40.0, -111.0)).unwrap();
        assert_eq!(j.track("g", 0, 9).unwrap().len(), 2, "the old points are still there");
        drop(j);
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn walk_recap(end_ms: i64) -> Recap {
        Recap { start_ms: end_ms - 600_000, end_ms, steps: 900, distance_m: 3100.0, quests_done: vec!["Oak".into()], ..Recap::default() }
    }

    #[test]
    fn a_kept_walk_roundtrips_and_is_scoped_to_game_and_end_time() {
        let j = Journal::open_memory().unwrap();
        let r = walk_recap(5_000_000);
        j.add_walk("g", &r).unwrap();
        assert_eq!(j.walk("g", 5_000_000).unwrap(), Some(r));
        assert_eq!(j.walk("other", 5_000_000).unwrap(), None);
        assert_eq!(j.walk("g", 5_000_001).unwrap(), None);
    }

    #[test]
    fn clearing_a_game_removes_its_walks() {
        let j = Journal::open_memory().unwrap();
        j.add_walk("g", &walk_recap(1)).unwrap();
        j.add_walk("keep", &walk_recap(1)).unwrap();
        j.clear_game("g").unwrap();
        assert_eq!(j.walk("g", 1).unwrap(), None);
        assert!(j.walk("keep", 1).unwrap().is_some());
    }

    #[test]
    fn an_unreadable_walk_row_is_an_error_and_the_newest_row_wins() {
        let j = Journal::open_memory().unwrap();
        j.conn.execute("INSERT INTO walks (game, end_ms, recap) VALUES ('g', 7, 'not json')", []).unwrap();
        assert!(j.walk("g", 7).is_err(), "a walk that cannot be read is not the same as no walk");
        let mut r = walk_recap(7);
        r.steps = 1;
        j.add_walk("g", &r).unwrap();
        assert_eq!(j.walk("g", 7).unwrap(), Some(r));
    }

    #[test]
    fn a_walk_kept_before_fields_were_added_still_opens() {
        let j = Journal::open_memory().unwrap();
        let old = r#"{"start_ms": 1, "end_ms": 7, "quests": [{"name": "Oak"}], "chains": [{"id": "1:step_up"}], "foragers": [{"name": "Acorns"}]}"#;
        j.conn.execute("INSERT INTO walks (game, end_ms, recap) VALUES ('g', 7, ?1)", [old]).unwrap();
        let r = j.walk("g", 7).unwrap().expect("the old walk opens");
        assert_eq!((r.start_ms, r.end_ms, r.steps), (1, 7, 0));
        assert_eq!((r.quests[0].name.as_str(), r.chains[0].id.as_str(), r.foragers[0].name.as_str()), ("Oak", "1:step_up", "Acorns"));
    }

    #[test]
    fn an_old_journal_gains_the_walks_table_on_open() {
        let dir = std::env::temp_dir().join(format!("apgo-journal-old-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("journal.sqlite");
        let _ = std::fs::remove_file(&path);
        Connection::open(&path)
            .unwrap()
            .execute_batch("CREATE TABLE events (id INTEGER PRIMARY KEY, game TEXT NOT NULL, t_ms INTEGER NOT NULL, kind TEXT NOT NULL, detail TEXT NOT NULL, lat REAL, lon REAL);")
            .unwrap();
        let j = Journal::open(&path).unwrap();
        j.add_walk("g", &walk_recap(3)).unwrap();
        assert!(j.walk("g", 3).unwrap().is_some());
        drop(j);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
