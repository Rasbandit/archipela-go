//! Readers for recorded walks: the debug raw track (`diag/raw/raw-NNNN.jsonl`) and, for older outings, the journal's points.

use std::path::Path;

use rusqlite::{Connection, OpenFlags};
use serde_json::Value;

use crate::loc::{CompassAccuracy, HeadingIn, Provider, RawFix};

/// What a recording holds, in time order.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Recording {
    /// Raw fixes.
    pub fixes: Vec<RawFix>,
    /// Step counter readings `(event ms, cumulative total)`.
    pub steps: Vec<(i64, i64)>,
    /// Compass readings.
    pub headings: Vec<HeadingIn>,
    /// Lines that could not be read.
    pub skipped: usize,
}

fn num(v: &Value, k: &str) -> Option<f64> {
    v.get(k).and_then(Value::as_f64)
}

fn int(v: &Value, k: &str) -> Option<i64> {
    v.get(k).and_then(Value::as_i64)
}

/// Add every `rawfix`, `rawsteps` and `rawhead` line of `text` to `rec` (other tags are ignored, unreadable lines counted).
pub fn parse_raw_lines(text: &str, rec: &mut Recording) {
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let Ok(v) = serde_json::from_str::<Value>(line) else {
            rec.skipped += 1;
            continue;
        };
        match v.get("tag").and_then(Value::as_str) {
            Some("rawfix") => {
                let (Some(t_ms), Some(lat), Some(lon)) = (int(&v, "tf"), num(&v, "lat"), num(&v, "lon")) else {
                    rec.skipped += 1;
                    continue;
                };
                rec.fixes.push(RawFix {
                    t_ms,
                    lat,
                    lon,
                    accuracy_m: num(&v, "acc").unwrap_or(1000.0),
                    speed_mps: num(&v, "spd"),
                    speed_acc_mps: num(&v, "spd_acc"),
                    bearing_deg: num(&v, "brg"),
                    bearing_acc_deg: num(&v, "brg_acc"),
                    altitude_m: num(&v, "alt"),
                    vertical_acc_m: num(&v, "valt"),
                    provider: Provider::parse(v.get("prov").and_then(Value::as_str).unwrap_or("other")),
                    mock: v.get("mock").and_then(Value::as_bool).unwrap_or(false),
                });
            }
            Some("rawsteps") => {
                if let (Some(te), Some(total)) = (int(&v, "te"), int(&v, "total")) {
                    rec.steps.push((te, total));
                }
            }
            Some("rawhead") => {
                // The sensor event time on the fix clock (`te`, as for steps), never the log time `t` (controller note, Task 14).
                if let (Some(t_ms), Some(az)) = (int(&v, "te"), num(&v, "az")) {
                    rec.headings.push(HeadingIn {
                        t_ms,
                        azimuth_deg: az,
                        accuracy: CompassAccuracy::parse(v.get("acc").and_then(Value::as_str).unwrap_or("")),
                        pitch_deg: num(&v, "pitch").unwrap_or(0.0),
                        roll_deg: num(&v, "roll").unwrap_or(0.0),
                        error_deg: num(&v, "err"),
                    });
                }
            }
            _ => {}
        }
    }
}

/// Every `raw-*.jsonl` file of `dir` (a pulled `diag/raw/`), oldest file first, then everything sorted by time.
///
/// # Errors
/// Returns a message if the directory cannot be listed.
pub fn read_raw_dir(dir: &Path) -> Result<Recording, String> {
    let mut files: Vec<_> = std::fs::read_dir(dir)
        .map_err(|e| format!("{}: {e}", dir.display()))?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with("raw-")) && p.extension().is_some_and(|e| e == "jsonl"))
        .collect();
    files.sort();
    let mut rec = Recording::default();
    for f in files {
        parse_raw_lines(&std::fs::read_to_string(&f).unwrap_or_default(), &mut rec);
    }
    rec.fixes.sort_by_key(|f| f.t_ms);
    rec.steps.sort_unstable();
    rec.headings.sort_by_key(|h| h.t_ms);
    Ok(rec)
}

/// The journal's real (not simulated) points in `from_ms..=to_ms`, as fused fixes with position, time and accuracy only. The file is opened
/// read-only, so a pulled journal is never changed.
///
/// # Errors
/// Returns a message if the file cannot be opened or read.
pub fn read_journal(path: &Path, from_ms: i64, to_ms: i64) -> Result<Vec<RawFix>, String> {
    let c = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX).map_err(|e| e.to_string())?;
    let mut st = c
        .prepare("SELECT t_ms, lat, lon, accuracy_m FROM points WHERE simulated = 0 AND t_ms BETWEEN ?1 AND ?2 ORDER BY t_ms, id")
        .map_err(|e| e.to_string())?;
    let rows = st.query_map((from_ms, to_ms), |r| Ok(RawFix::at(r.get(1)?, r.get(2)?, r.get(0)?, r.get(3)?))).map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::journal::{Journal, TrackPoint};

    const LINES: &str = concat!(
        r#"{"t":5,"lvl":"I","tag":"rawfix","msg":"","tf":1000,"ert":7,"lat":40.0,"lon":-111.0,"acc":4.5,"spd":1.2,"spd_acc":0.5,"brg":90.0,"brg_acc":10.0,"alt":null,"valt":null,"prov":"gps","mock":false}"#,
        "\n",
        r#"{"t":6,"lvl":"I","tag":"rawsteps","msg":"","total":1234,"te":1500}"#,
        "\n",
        r#"{"t":7,"lvl":"I","tag":"rawhead","msg":"","te":1700,"az":270.0,"acc":"high","pitch":12.0,"roll":-3.0}"#,
        "\n",
        "not json\n",
        r#"{"t":8,"lvl":"I","tag":"rawstate","msg":"","presence":"InZone","counting":true,"zone":"inside","app_visible":true}"#,
        "\n"
    );

    #[test]
    fn raw_lines_become_fixes_steps_and_headings_and_junk_is_counted() {
        let mut r = Recording::default();
        parse_raw_lines(LINES, &mut r);
        assert_eq!(r.fixes.len(), 1);
        let f = r.fixes[0];
        assert_eq!((f.t_ms, f.provider, f.speed_mps, f.bearing_acc_deg, f.altitude_m), (1000, Provider::Gps, Some(1.2), Some(10.0), None));
        assert_eq!(r.steps, vec![(1500, 1234)]);
        let h = r.headings[0];
        assert_eq!((h.azimuth_deg, h.accuracy, h.pitch_deg, h.roll_deg), (270.0, CompassAccuracy::High, 12.0, -3.0));
        assert_eq!(h.t_ms, 1700, "a heading is on the sensor event clock (te) like steps, not the log time (t)");
        assert_eq!(r.skipped, 1, "the line that is not JSON");
    }

    #[test]
    fn a_heading_line_keeps_the_phones_heading_error_when_it_has_one() {
        let mut r = Recording::default();
        parse_raw_lines(LINES, &mut r);
        parse_raw_lines(r#"{"t":9,"tag":"rawhead","te":1800,"az":10.0,"acc":"medium","pitch":1.0,"roll":2.0,"err":12.5}"#, &mut r);
        assert_eq!(r.headings[0].error_deg, None, "the rotation vector has no error");
        assert_eq!(r.headings[1].error_deg, Some(12.5));
    }

    #[test]
    fn a_fix_without_accuracy_reads_as_unusably_coarse() {
        let mut r = Recording::default();
        parse_raw_lines(r#"{"t":5,"tag":"rawfix","tf":1,"lat":1.0,"lon":2.0,"acc":null,"prov":"fused","mock":false}"#, &mut r);
        assert!(r.fixes[0].accuracy_m > 100.0);
    }

    #[test]
    fn journal_points_in_the_window_are_read_without_simulated_ones_and_the_file_is_left_alone() {
        let dir = std::env::temp_dir().join(format!("apgo-record-journal-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("journal.db");
        {
            let j = Journal::open(&path).unwrap();
            for (t, sim) in [(1000, false), (2000, true), (3000, false), (9000, false)] {
                j.add_point("g", &TrackPoint { t_ms: t, lat: 40.0, lon: -111.0, accuracy_m: 5.0, simulated: sim }).unwrap();
            }
        }
        let before = std::fs::metadata(&path).unwrap().modified().unwrap();
        let fixes = read_journal(&path, 0, 5000).unwrap();
        assert_eq!(fixes.iter().map(|f| f.t_ms).collect::<Vec<_>>(), [1000, 3000]);
        assert_eq!(std::fs::metadata(&path).unwrap().modified().unwrap(), before, "opened read-only");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
