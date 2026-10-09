//! `UniFFI` surface of apgo-core. Blocking calls: invoke from a background thread on the host side.

use std::path::PathBuf;

use apgo_core::fill::{fetch_streets, lattice};
use apgo_core::geo::Point;
use apgo_core::num::count_u32;
use apgo_core::sampler::{sample, TripSpec};
use apgo_core::zone::Zone;

uniffi::setup_scaffolding!();

pub mod engine;

/// Must match the apworld's game name exactly (`apworld/ap_go2/constants.py`).
const GAME_NAME: &str = "Archipela-Go 2: Electric Boogaloo";

/// A WGS84 coordinate in degrees.
#[derive(Debug, uniffi::Record)]
pub struct GeoPoint {
    /// Latitude in degrees.
    pub lat: f64,
    /// Longitude in degrees.
    pub lon: f64,
}

/// Where the points for trips come from.
#[derive(Debug, uniffi::Enum)]
pub enum FillMode {
    /// Offline lattice of points: works anywhere with no map data.
    Cells,
    /// Streets and paths from OpenStreetMap (one bulk request, cached).
    Streets,
}

/// A trip with its chosen place.
#[derive(Debug, uniffi::Record)]
pub struct TripOut {
    /// Trip number, starting at 1.
    pub number: u32,
    /// Archipelago location id the trip is for (0 when not assigned).
    pub location_id: i64,
    /// Effort tier, starting at 1.
    pub tier: u8,
    /// Latitude of the chosen place in degrees.
    pub lat: f64,
    /// Longitude of the chosen place in degrees.
    pub lon: f64,
    /// Name of the chosen place.
    pub name: String,
    /// Straight-line distance from home in metres.
    pub distance_m: f64,
    /// False when no place fit the tier band and the nearest one was used.
    pub in_band: bool,
}

/// An error from the core, as text.
#[derive(Debug, uniffi::Error)]
pub enum CoreError {
    /// The operation failed.
    Failed {
        /// What went wrong.
        detail: String,
    },
}

/// What a trip needs from the sampler.
#[derive(Debug, uniffi::Record)]
pub struct TripSpecIn {
    /// Archipelago location id the trip is for.
    pub location_id: i64,
    /// Effort tier wanted.
    pub tier: u8,
}

/// The area to fill with trips.
#[derive(Debug, uniffi::Enum)]
pub enum ZoneIn {
    /// Circle around `center`; `step_m` is the effective meters per tier (radius = 10 steps).
    /// A circle around `center`.
    Circle {
        /// Middle of the circle.
        center: GeoPoint,
        /// Effective metres per tier; the radius is ten steps.
        step_m: f64,
    },
    /// Any drawn polygon (3+ vertices); tiers are tenths of the polygon's extent from its center.
    /// A drawn polygon.
    Polygon {
        /// Corner points, at least three.
        vertices: Vec<GeoPoint>,
    },
}

/// Fill a zone for the trips the apworld assigned (`slot_data.trips`): each gets a point in its tier band.
///
/// # Errors
/// Returns an error if a polygon has fewer than three points, or the street request fails.
#[uniffi::export]
#[allow(clippy::needless_pass_by_value)] // uniffi requires owned args
pub fn generate_trips_for(zone: ZoneIn, specs: Vec<TripSpecIn>, seed: u64, mode: FillMode, cache_dir: String) -> Result<Vec<TripOut>, CoreError> {
    let (zone, step_m) = match zone {
        ZoneIn::Circle { center, step_m } => (Zone::Circle { center: Point::new(center.lat, center.lon), radius_m: step_m * 10.0 }, step_m),
        ZoneIn::Polygon { vertices } => {
            if vertices.len() < 3 {
                return Err(CoreError::Failed { detail: "a zone needs at least 3 points".into() });
            }
            let z = Zone::Polygon(vertices.iter().map(|v| Point::new(v.lat, v.lon)).collect());
            let step = z.max_extent_m() / 10.0;
            (z, step)
        }
    };
    let candidates = match mode {
        FillMode::Cells => lattice(&zone, (step_m / 3.0).clamp(30.0, 150.0)),
        FillMode::Streets => fetch_streets(&zone, 50.0, Some(&PathBuf::from(cache_dir))).map_err(|e| CoreError::Failed { detail: e.to_string() })?,
    };
    let core_specs: Vec<TripSpec> = specs.iter().enumerate().map(|(i, s)| TripSpec { number: count_u32(i) + 1, tier: s.tier }).collect();
    let out = sample(&candidates, zone.home(), &core_specs, step_m, 40.0, seed);
    Ok(out
        .into_iter()
        .map(|t| TripOut {
            number: t.number,
            location_id: specs[(t.number - 1) as usize].location_id,
            tier: t.tier,
            lat: t.candidate.point.lat,
            lon: t.candidate.point.lon,
            name: t.candidate.name,
            distance_m: t.distance_m,
            in_band: t.in_band,
        })
        .collect())
}

impl std::fmt::Display for CoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Failed { detail } => write!(f, "{detail}"),
        }
    }
}

impl std::error::Error for CoreError {}

/// The core version as text.
#[uniffi::export]
#[must_use]
pub fn core_version() -> String {
    format!("apgo-core {}", env!("CARGO_PKG_VERSION"))
}

/// Fill a circular zone around `center` with `trips` trips (tiers cycle 1..=10).
///
/// # Errors
/// Returns an error if the street request fails.
#[uniffi::export]
#[allow(clippy::needless_pass_by_value)] // uniffi requires owned args
pub fn generate_trips(center: GeoPoint, radius_m: f64, trips: u32, seed: u64, mode: FillMode, cache_dir: String) -> Result<Vec<TripOut>, CoreError> {
    let zone = Zone::Circle { center: Point::new(center.lat, center.lon), radius_m };
    let candidates = match mode {
        FillMode::Cells => lattice(&zone, 150.0),
        FillMode::Streets => fetch_streets(&zone, 50.0, Some(&PathBuf::from(cache_dir))).map_err(|e| CoreError::Failed { detail: e.to_string() })?,
    };
    let specs: Vec<TripSpec> = (1..=trips).map(|n| TripSpec { number: n, tier: ((n - 1) % 10) as u8 + 1 }).collect();
    let out = sample(&candidates, zone.home(), &specs, zone.max_extent_m() / 10.0, 75.0, seed);
    Ok(out
        .into_iter()
        .map(|t| TripOut {
            number: t.number,
            location_id: 0,
            tier: t.tier,
            lat: t.candidate.point.lat,
            lon: t.candidate.point.lon,
            name: t.candidate.name,
            distance_m: t.distance_m,
            in_band: t.in_band,
        })
        .collect())
}

// ---------------------------------------------------------------------------------------------
// Archipelago session (non-blocking: the host polls on a timer; the crate does the I/O)
// ---------------------------------------------------------------------------------------------

use std::sync::{Arc, Mutex};

use archipelago_rs as ap;

/// An item received from the multiworld.
#[derive(Debug, uniffi::Record)]
pub struct ReceivedItemOut {
    /// Position in the list of received items.
    pub index: u32,
    /// Archipelago item id.
    pub item_id: i64,
    /// Item name.
    pub name: String,
    /// Name of the player who sent the item.
    pub sender: String,
    /// Whether the item is needed to progress.
    pub progression: bool,
    /// Whether the item is a trap.
    pub trap: bool,
}

/// Something that happened on the Archipelago connection.
#[derive(Debug, uniffi::Enum)]
pub enum ApEvent {
    /// The connection is established.
    Connected,
    /// New items arrived.
    ReceivedItems {
        /// Index of the first new item in the received list.
        from_index: u32,
    },
    /// A message from the server.
    Print {
        /// The message text.
        text: String,
    },
    /// Server-side data changed.
    Updated,
    /// The connection reported an error.
    Error {
        /// What went wrong.
        detail: String,
    },
    /// Something the app does not need to react to.
    Other,
}

/// A connection to an Archipelago server, polled by the host on a timer.
#[derive(uniffi::Object)]
pub struct ApSession {
    conn: Mutex<ap::Connection>,
}

#[uniffi::export]
#[allow(clippy::needless_pass_by_value)] // uniffi requires owned args
impl ApSession {
    /// Start connecting (returns immediately). `url` like `localhost:38281` or `wss://archipelago.gg:38281`.
    #[uniffi::constructor]
    pub fn connect(url: String, slot: String, password: Option<String>, cache_dir: String) -> Arc<Self> {
        // Two rustls backends are linked (ring + aws-lc-rs); pick one explicitly or rustls panics.
        static CRYPTO: std::sync::Once = std::sync::Once::new();
        CRYPTO.call_once(|| {
            let _ = rustls::crypto::ring::default_provider().install_default();
        });
        let mut options = ap::ConnectionOptions::new()
            .receive_items(ap::ItemHandling::OtherWorlds { own_world: true, starting_inventory: true })
            .cache(ap::Cache::path(cache_dir))
            .tags(vec!["Archipela-Go2"]);
        if let Some(p) = password {
            options = options.password(p);
        }
        Arc::new(Self { conn: Mutex::new(ap::Connection::new(&url, &slot, Some(GAME_NAME), options)) })
    }

    /// Drain pending network events. Call every few hundred ms.
    pub fn poll(&self) -> Vec<ApEvent> {
        let mut conn = self.conn.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        conn.update()
            .into_iter()
            .map(|e| match e {
                ap::Event::Connected => ApEvent::Connected,
                ap::Event::ReceivedItems(i) => ApEvent::ReceivedItems { from_index: count_u32(i) },
                ap::Event::Print(p) => ApEvent::Print { text: p.to_string() },
                ap::Event::Updated(_) => ApEvent::Updated,
                ap::Event::Error(err) => ApEvent::Error { detail: err.to_string() },
                _ => ApEvent::Other,
            })
            .collect()
    }

    /// `connecting`, `connected`, or `disconnected: <reason>`.
    pub fn status(&self) -> String {
        let conn = self.conn.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        match conn.state() {
            ap::ConnectionState::Connecting(_) => "connecting".into(),
            ap::ConnectionState::Connected(_) => "connected".into(),
            ap::ConnectionState::Disconnected(err) => format!("disconnected: {err}"),
        }
    }

    /// The `slot_data` the apworld sent (JSON), once connected.
    pub fn slot_data_json(&self) -> Option<String> {
        let conn = self.conn.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        conn.client().map(|c| c.slot_data().to_string())
    }

    /// Every item received so far, in order.
    #[allow(clippy::significant_drop_tightening)] // the client borrows from the connection guard until the result is built
    pub fn received_items(&self) -> Vec<ReceivedItemOut> {
        let conn = self.conn.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let Some(client) = conn.client() else { return vec![] };
        client
            .received_items()
            .iter()
            .map(|r| {
                let item = r.item();
                ReceivedItemOut {
                    index: count_u32(r.index()),
                    item_id: item.id(),
                    name: item.name().to_string(),
                    sender: r.sender().name().to_string(),
                    progression: r.is_progression(),
                    trap: r.is_trap(),
                }
            })
            .collect()
    }

    /// Location ids the server already has as checked (use after reconnecting).
    #[allow(clippy::significant_drop_tightening)] // the client borrows from the connection guard until the result is built
    pub fn checked_location_ids(&self) -> Vec<i64> {
        let conn = self.conn.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let Some(client) = conn.client() else { return vec![] };
        client.checked_locations().map(|l| l.id()).collect()
    }

    /// Tell the server this slot has reached its goal (shows as complete for the whole multiworld).
    ///
    /// # Errors
    /// Returns an error if the session is not connected or the server rejects the update.
    #[allow(clippy::significant_drop_tightening)] // the client borrows from the connection guard until the result is built
    pub fn send_goal(&self) -> Result<(), CoreError> {
        let mut conn = self.conn.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let client = conn.client_mut().ok_or_else(|| CoreError::Failed { detail: "not connected".into() })?;
        client.set_status(ap::ClientStatus::Goal).map_err(|e| CoreError::Failed { detail: e.to_string() })
    }

    /// Tell the server this location was checked.
    ///
    /// # Errors
    /// Returns an error if the session is not connected or the server rejects the update.
    #[allow(clippy::significant_drop_tightening)] // the client borrows from the connection guard until the result is built
    pub fn send_check(&self, location_id: i64) -> Result<(), CoreError> {
        let mut conn = self.conn.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let client = conn.client_mut().ok_or_else(|| CoreError::Failed { detail: "not connected".into() })?;
        client.mark_checked([location_id]).map_err(|e| CoreError::Failed { detail: e.to_string() })
    }
}

/// How long the host waits between Archipelago polls (see `apgo_core::ap_poll`): fast right after the server said something,
/// slower while it is quiet. The host keeps one per session.
#[derive(uniffi::Object)]
pub struct ApPoll {
    backoff: Mutex<apgo_core::ap_poll::PollBackoff>,
}

#[uniffi::export]
impl ApPoll {
    /// A fresh back-off, starting fast.
    #[uniffi::constructor]
    #[must_use]
    pub fn new() -> Arc<Self> {
        Arc::new(Self { backoff: Mutex::new(apgo_core::ap_poll::PollBackoff::default()) })
    }

    /// The wait before the next poll, in milliseconds, given whether this poll brought any events.
    pub fn next_delay_ms(&self, active: bool) -> u64 {
        self.backoff.lock().unwrap_or_else(std::sync::PoisonError::into_inner).next(active)
    }
}

/// Whether the open game must sync with the Archipelago server (see `apgo_core::ap_poll::needs_sync`).
#[uniffi::export]
#[allow(clippy::needless_pass_by_value)] // uniffi requires owned args
#[must_use]
pub fn ap_needs_sync(server_changed: bool, synced_game: Option<String>, open_game: Option<String>) -> bool {
    apgo_core::ap_poll::needs_sync(server_changed, synced_game.as_deref(), open_game.as_deref())
}
