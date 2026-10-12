//! Archipelago client compliance: our `ApSession` against the real, pinned Archipelago server (`.ap/MultiServer.py`, our apworld,
//! slot `Tester`), one test per rule from Archipelago's client guide (`.ap/docs/adding games.md`, "Hard Requirements") and network
//! protocol (`.ap/docs/network protocol.md`), plus the server behaviour our battery work relies on.
//!
//! Needs `just setup-ap`. Run: `just test-ap` (hosts its own server on :38291 in `.ap/compliance`, never the dev one).
//! Ignored in the normal test run: it starts a Python server.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code: a failed step is a failed test

use std::path::PathBuf;
use std::process::Command;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use apgo_ffi::{ApEvent, ApSession};

const PORT: u16 = 38291;
const SLOT: &str = "Tester";

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().unwrap()
}

// Run scripts/ap_host.sh with the compliance folder and port.
fn host(args: &[&str]) {
    let out = Command::new("bash")
        .arg(root().join("scripts/ap_host.sh"))
        .args(args)
        .env("APGO_AP_DIR", root().join(".ap/compliance"))
        .env("APGO_AP_PORT", PORT.to_string())
        .env("APGO_AP_NO_ADB", "1")
        .output()
        .expect("scripts/ap_host.sh runs");
    assert!(out.status.success(), "ap_host.sh {args:?} failed:\n{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
}

// The server, generated once per run; tests take turns (a restart test stops it).
fn server() -> std::sync::MutexGuard<'static, ()> {
    static SEED: OnceLock<()> = OnceLock::new();
    static TURN: Mutex<()> = Mutex::new(());
    let turn = TURN.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    SEED.get_or_init(|| host(&["start", "10"]));
    if std::net::TcpStream::connect(("127.0.0.1", PORT)).is_err() {
        host(&["serve"]);
    }
    turn
}

fn cache_dir() -> String {
    let d = std::env::temp_dir().join(format!("apgo-compliance-{}", std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    d.to_string_lossy().into_owned()
}

// Drain `s` the way the app does (wait for data, then poll) until `done` or `secs` pass; every event seen is returned.
fn drain_until(s: &ApSession, secs: u64, mut done: impl FnMut(&ApSession, &[ApEvent]) -> bool) -> Vec<ApEvent> {
    let end = Instant::now() + Duration::from_secs(secs);
    let mut seen = Vec::new();
    while Instant::now() < end {
        seen.extend(s.poll());
        if done(s, &seen) {
            return seen;
        }
        if !s.wait_for_data(200) {
            std::thread::sleep(Duration::from_millis(100)); // connecting: no socket to wait on yet
        }
    }
    panic!("not done within {secs} s; status {}, events {seen:?}", s.status());
}

// A connected session to the compliance server; `url` without a scheme makes the client try wss, then ws.
fn connect_to(url: &str, slot: &str) -> Arc<ApSession> {
    let s = ApSession::connect(url.into(), slot.into(), None, cache_dir());
    drain_until(&s, 30, |s, _| s.status() == "connected");
    s
}

fn connect() -> Arc<ApSession> {
    connect_to(&format!("localhost:{PORT}"), SLOT)
}

// The slot's quest location ids, from slot_data.
fn location_ids(s: &ApSession) -> Vec<i64> {
    let data: serde_json::Value = serde_json::from_str(&s.slot_data_json().expect("slot_data once connected")).unwrap();
    data["trips"].as_array().expect("slot_data.trips").iter().map(|t| t["location_id"].as_i64().unwrap()).collect()
}

// Location ids not yet checked on the server.
fn unchecked(s: &ApSession, n: usize) -> Vec<i64> {
    let done = s.checked_location_ids();
    let ids: Vec<i64> = location_ids(s).into_iter().filter(|id| !done.contains(id)).take(n).collect();
    assert_eq!(ids.len(), n, "the seed has {n} unchecked locations left (rerun after `rm -rf .ap/compliance`)");
    ids
}

/// Hard requirement: "Handle both secure and unsecure websocket connections". An address without a scheme tries `wss://` first
/// and falls back to `ws://` (as Archipelago's own client does), then completes the handshake (`RoomInfo`, `Connect`, `Connected`).
#[test]
#[ignore = "needs the pinned Archipelago server: just test-ap"]
fn an_address_without_a_scheme_falls_back_to_ws_and_connects() {
    let _server = server();
    let s = connect();
    assert!(s.slot_data_json().is_some(), "Connected carries slot_data");
    let plain = connect_to(&format!("ws://localhost:{PORT}"), SLOT);
    assert_eq!(plain.status(), "connected", "an explicit ws:// works too");
}

/// Protocol: a client sends `LocationChecks`; repeats are harmless. We re-send every finished check on each connect, so the
/// server must take repeats and checks it already has without complaint, and keep them.
#[test]
#[ignore = "needs the pinned Archipelago server: just test-ap"]
fn checks_reach_the_server_and_sending_them_again_is_harmless() {
    let _server = server();
    let s = connect();
    let ids = unchecked(&s, 2);
    s.send_checks(ids.clone()).unwrap();
    s.send_checks(ids.clone()).unwrap(); // the reconnect re-send: everything again
    drain_until(&s, 10, |s, _| ids.iter().all(|id| s.checked_location_ids().contains(id)));
    let again = connect();
    assert!(ids.iter().all(|id| again.checked_location_ids().contains(id)), "a new session sees them: the server kept them");
    assert_eq!(again.status(), "connected", "repeats did not get the client dropped");
}

/// Hard requirement: "keep an index for items received in order to resync"; protocol: `ReceivedItems` items are one list with a
/// guaranteed order. Our own world's items come back for our own checks (`items_handling` `own_world`): the indexes run 0, 1, 2, ...
#[test]
#[ignore = "needs the pinned Archipelago server: just test-ap"]
fn received_items_carry_a_gapless_index() {
    let _server = server();
    let s = connect();
    let before = s.received_items().len();
    s.send_checks(unchecked(&s, 2)).unwrap();
    drain_until(&s, 10, |s, _| s.received_items().len() > before);
    let indexes: Vec<u32> = s.received_items().iter().map(|i| i.index).collect();
    assert_eq!(indexes, (0..u32::try_from(indexes.len()).unwrap()).collect::<Vec<_>>(), "gapless from 0");
}

/// Hard requirement: "Send a status update packet alerting the server that the player has completed their goal". The server
/// announces it to the room.
#[test]
#[ignore = "needs the pinned Archipelago server: just test-ap"]
fn the_goal_status_update_is_accepted() {
    let _server = server();
    let s = connect();
    s.send_goal().unwrap();
    let said = |e: &ApEvent| matches!(e, ApEvent::Print { text } if text.contains("goal"));
    drain_until(&s, 10, |_, seen| seen.iter().any(said));
}

/// Hard requirement: "Reconnect if the connection is unstable and lost while playing". A server restart drops the session (it
/// reports disconnected, which the app turns into a reconnect), and a new session gets the full item list again (index 0), so
/// nothing received before is lost.
#[test]
#[ignore = "needs the pinned Archipelago server: just test-ap"]
fn a_server_restart_drops_the_session_and_a_new_one_gets_every_item_again() {
    let _server = server();
    let s = connect();
    s.send_checks(unchecked(&s, 1)).unwrap();
    drain_until(&s, 10, |s, _| !s.received_items().is_empty());
    let had = s.received_items().len();
    host(&["stop"]);
    drain_until(&s, 15, |s, _| s.status().starts_with("disconnected"));
    host(&["serve"]);
    let again = connect();
    drain_until(&again, 10, |s, _| s.received_items().len() >= had);
    assert_eq!(again.received_items().len(), had, "every item again, none twice");
}

/// A first connect to a slot that does not exist is refused, never "connected": the app only auto-reconnects sessions that were
/// connected (a refusal is the player's to fix, not a loop).
#[test]
#[ignore = "needs the pinned Archipelago server: just test-ap"]
fn a_wrong_slot_is_refused_and_never_connected() {
    let _server = server();
    let s = ApSession::connect(format!("localhost:{PORT}"), "Nobody".into(), None, cache_dir());
    let seen = drain_until(&s, 30, |s, _| s.status().starts_with("disconnected"));
    assert!(!seen.iter().any(|e| matches!(e, ApEvent::Connected)), "never connected: {seen:?}");
}

/// What the battery work relies on: the stock server pings a quiet connection every 20 s (websockets defaults), so the app may
/// sleep in the socket and read 60 s of silence as a dead connection.
#[test]
#[ignore = "needs the pinned Archipelago server: just test-ap"]
fn the_server_pings_a_quiet_connection_within_twenty_five_seconds() {
    let _server = server();
    let s = connect();
    drain_until(&s, 5, |_, _| true);
    let _ = s.poll(); // nothing left to read
    let t = Instant::now();
    assert!(s.wait_for_data(25_000), "no data for 25 s: the server no longer pings, so 60 s of silence is not proof of a dead socket");
    assert!(t.elapsed() < Duration::from_secs(25));
    assert_eq!(s.poll().len(), 0, "a ping is no event");
    assert_eq!(s.status(), "connected", "and the client answered it (tungstenite pongs while reading)");
}

/// For servers that do not ping: a probe (`Get`) is always answered, so a live but quiet connection wakes before the app would
/// reconnect it.
#[test]
#[ignore = "needs the pinned Archipelago server: just test-ap"]
fn a_probe_is_answered() {
    let _server = server();
    let s = connect();
    drain_until(&s, 5, |_, _| true);
    let _ = s.poll();
    // Right after a server ping the next one is ~20 s away, so a wake within 3 s of the probe is its answer, not a ping.
    assert!(s.wait_for_data(25_000), "a ping first");
    let _ = s.poll();
    s.probe().unwrap();
    assert!(s.wait_for_data(3_000), "the server answered the Get");
}

/// A game is stamped with its server's `seed/slot` and only exchanges checks, goal and items with that server (a goal sent to
/// another seed could release that whole world). The identity names this seed and slot, and stays the same across a restart.
#[test]
#[ignore = "needs the pinned Archipelago server: just test-ap"]
fn the_server_identity_is_seed_and_slot_and_survives_a_restart() {
    let _server = server();
    let s = connect();
    let id = s.server_identity().expect("known once connected");
    assert!(id.ends_with(&format!("/{SLOT}")) && id.len() > SLOT.len() + 1, "seed/slot: {id}");
    host(&["serve"]);
    assert_eq!(connect().server_identity(), Some(id), "same seed and slot after a restart: the game keeps its server");
}
