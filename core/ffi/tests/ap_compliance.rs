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
// Where the moved-room test serves the same seed (an archipelago.gg room that lost its port).
const MOVED_PORT: u16 = 38292;
const SLOT: &str = "Tester";

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().unwrap()
}

// Run scripts/ap_host.sh with the compliance folder and port.
fn host(args: &[&str]) {
    host_on(PORT, args);
}

// Run scripts/ap_host.sh with the compliance folder on `port`.
fn host_on(port: u16, args: &[&str]) {
    let out = Command::new("bash")
        .arg(root().join("scripts/ap_host.sh"))
        .args(args)
        .env("APGO_AP_DIR", root().join(".ap/compliance"))
        .env("APGO_AP_PORT", port.to_string())
        .env("APGO_AP_NO_ADB", "1")
        // A goal must not release or collect the seed: the tests that need unchecked locations would find none left.
        .env("APGO_AP_SERVER_ARGS", "--release_mode disabled --collect_mode disabled")
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

// Drain `s` the way the app does for `time`; every event seen is returned.
fn drain_for(s: &ApSession, time: Duration) -> Vec<ApEvent> {
    let end = Instant::now() + time;
    let mut seen = s.poll();
    while Instant::now() < end {
        s.wait_for_data(100);
        seen.extend(s.poll());
    }
    seen
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
    assert_eq!(ids.len(), n, "the seed has {n} unchecked locations left (each run generates a fresh one with 10 trips)");
    ids
}

// Check one more location and wait until the server lists it and its item is in. The server sends the two packets
// independently (`register_location_checks`), and in this one-player seed with no start inventory every checked location gives
// Tester exactly one item. Waiting for "some items" instead races with items from earlier tests' checks.
fn check_one(s: &ApSession) {
    let id = unchecked(s, 1)[0];
    s.send_checks(vec![id]).unwrap();
    drain_until(s, 10, |s, _| {
        let checked = s.checked_location_ids();
        checked.contains(&id) && s.received_items().len() >= checked.len()
    });
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
    check_one(&s);
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
    let events = s.poll();
    assert!(events.is_empty(), "a ping is no event: {events:?}");
    assert_eq!(s.status(), "connected", "and the client answered it (soketto pongs while reading)");
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

/// Hard requirement: "Reconnect if the connection is unstable and lost while playing", across an app restart too. A game saves the
/// address and slot it connected with (`Engine::ap_saved_server`); after the process dies, a new session from those alone (no
/// player input) is the same seed and slot and gets every item again.
#[test]
#[ignore = "needs the pinned Archipelago server: just test-ap"]
fn a_restarted_app_reconnects_from_the_saved_address_and_gets_its_items() {
    let _server = server();
    let (address, slot) = (format!("localhost:{PORT}"), SLOT);
    let s = connect_to(&address, slot);
    check_one(&s);
    let (id, had) = (s.server_identity(), s.received_items().len());
    drop(s); // the app is killed: nothing but the saved game is left
    let resumed = connect_to(&address, slot);
    drain_until(&resumed, 10, |s, _| s.received_items().len() >= had);
    assert_eq!(resumed.server_identity(), id, "the same game on the server");
    assert_eq!(resumed.received_items().len(), had, "every item again, none twice");
}

/// Hard requirement: "Be able to change the port for saved connection info" (rooms on archipelago.gg can lose their port). The
/// same seed served on another port is the same seed and slot, so a game pointed at the new address carries on there.
#[test]
#[ignore = "needs the pinned Archipelago server: just test-ap"]
fn a_room_moved_to_another_port_is_the_same_game() {
    let _server = server();
    let s = connect();
    let id = s.server_identity();
    host(&["stop"]);
    host_on(MOVED_PORT, &["serve"]);
    let moved = connect_to(&format!("localhost:{MOVED_PORT}"), SLOT);
    let same = moved.server_identity();
    let checked = moved.send_checks(unchecked(&moved, 1));
    host_on(MOVED_PORT, &["stop"]); // the next test serves on PORT again
    assert_eq!(same, id, "same seed and slot on the new port: the game keeps exchanging");
    checked.unwrap();
}

/// Network protocol: "Websocket connections should support per-message compression. Uncompressed connections are deprecated and
/// may stop working in the future." The server warns every client that connects without permessage-deflate; ours never gets it.
#[test]
#[ignore = "needs the pinned Archipelago server: just test-ap"]
fn the_connection_is_compressed_so_the_server_never_warns() {
    let _server = server();
    let s = ApSession::connect(format!("localhost:{PORT}"), SLOT.into(), None, cache_dir());
    let mut seen = drain_until(&s, 30, |s, _| s.status() == "connected");
    seen.extend(drain_for(&s, Duration::from_secs(2))); // the warning follows `Connected`
    let warned: Vec<_> = seen.iter().filter(|e| matches!(e, ApEvent::Print { text } if text.contains("compressed"))).collect();
    assert!(warned.is_empty(), "the server saw an uncompressed connection: {warned:?}");
}

// A TCP proxy to the compliance server that can be frozen: it then passes nothing either way but keeps both sockets open, as a
// dead network (or a hung server) looks to the client, and the server never gets what the client sent.
struct Proxy {
    port: u16,
    frozen: Arc<std::sync::atomic::AtomicBool>,
    // Both sockets, shut down when the proxy is dropped so the server sees the client leave (a frozen client it still holds would
    // stall it until its ping timeout, into the next test).
    sockets: Arc<Mutex<Vec<std::net::TcpStream>>>,
}

// Copy `from` to `to` until `frozen`, then hold both open and pass nothing more.
fn pipe(mut from: std::net::TcpStream, mut to: std::net::TcpStream, frozen: Arc<std::sync::atomic::AtomicBool>) {
    use std::io::{Read, Write};
    use std::sync::atomic::Ordering;
    std::thread::spawn(move || {
        let mut buf = [0u8; 16 * 1024];
        while !frozen.load(Ordering::SeqCst) {
            match from.read(&mut buf) {
                Ok(0) | Err(_) => return,
                Ok(n) if !frozen.load(Ordering::SeqCst) && to.write_all(&buf[..n]).is_ok() => {}
                Ok(_) => break,
            }
        }
        std::thread::park(); // frozen: keep the sockets, read nothing
        drop((from, to));
    });
}

impl Proxy {
    fn start() -> Self {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let frozen = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let stop = frozen.clone();
        let sockets = Arc::new(Mutex::new(Vec::new()));
        let held = sockets.clone();
        std::thread::spawn(move || {
            let (client, _) = listener.accept().unwrap();
            let server = std::net::TcpStream::connect(("127.0.0.1", PORT)).unwrap();
            held.lock().unwrap().extend([client.try_clone().unwrap(), server.try_clone().unwrap()]);
            pipe(client.try_clone().unwrap(), server.try_clone().unwrap(), stop.clone());
            pipe(server, client, stop);
        });
        Self { port, frozen, sockets }
    }

    fn freeze(&self) {
        self.frozen.store(true, std::sync::atomic::Ordering::SeqCst);
    }
}

impl Drop for Proxy {
    fn drop(&mut self) {
        for s in self.sockets.lock().unwrap_or_else(std::sync::PoisonError::into_inner).iter() {
            let _ = s.shutdown(std::net::Shutdown::Both);
        }
    }
}

/// Robustness: a send to a server that stopped reading (a hung server, a dead network) fills the socket and would block, holding
/// the session (the app's whole Archipelago loop) until the OS gave up on the connection many minutes later. It fails within
/// seconds instead, and the session reports disconnected, so the app reconnects.
#[test]
#[ignore = "needs the pinned Archipelago server: just test-ap"]
fn a_send_to_a_server_that_stopped_reading_fails_in_seconds_and_disconnects() {
    let _server = server();
    let proxy = Proxy::start();
    let s = connect_to(&format!("ws://127.0.0.1:{}", proxy.port), SLOT);
    proxy.freeze();
    // Every location in a shuffled order compresses poorly: tens of KB per message, so a few hundred fill the socket buffers.
    let ids = location_ids(&s);
    let many: Vec<i64> = (0..120_000_usize).map(|i| ids[(i * 7 + i / 3 + i * i % 11) % ids.len()]).collect();
    let (tx, rx) = std::sync::mpsc::channel();
    let sender = s.clone();
    let t = Instant::now();
    std::thread::spawn(move || {
        let failed = (0..5_000).any(|_| sender.send_checks(many.clone()).is_err());
        let _ = tx.send(failed);
    });
    let failed = rx.recv_timeout(Duration::from_secs(60)).expect("a send blocked on the stopped server for 60 s");
    assert!(failed, "the socket never filled: send more");
    assert!(t.elapsed() < Duration::from_secs(45), "took {:?}", t.elapsed());
    drain_until(&s, 5, |s, _| s.status().starts_with("disconnected"));
}
