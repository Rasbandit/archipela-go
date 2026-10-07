//! UniFFI surface of apgo-core. Blocking calls: invoke from a background thread on the host side.

use std::path::PathBuf;

use apgo_core::fill::{fetch_streets, lattice};
use apgo_core::geo::Point;
use apgo_core::sampler::{sample, TripSpec};
use apgo_core::zone::Zone;

uniffi::setup_scaffolding!();

/// Must match the apworld's game name exactly (apworld/ap_go2/constants.py).
const GAME_NAME: &str = "Archipela-Go 2: Electric Boogaloo";

#[derive(Debug, uniffi::Record)]
pub struct GeoPoint {
    pub lat: f64,
    pub lon: f64,
}

#[derive(Debug, uniffi::Enum)]
pub enum FillMode {
    /// Offline lattice of points: works anywhere with no map data.
    Cells,
    /// Streets and paths from OpenStreetMap (one bulk request, cached).
    Streets,
}

#[derive(Debug, uniffi::Record)]
pub struct TripOut {
    pub number: u32,
    pub location_id: i64,
    pub tier: u8,
    pub lat: f64,
    pub lon: f64,
    pub name: String,
    pub distance_m: f64,
    pub in_band: bool,
}

#[derive(Debug, uniffi::Error)]
pub enum CoreError {
    Failed { detail: String },
}

#[derive(Debug, uniffi::Record)]
pub struct TripSpecIn {
    pub location_id: i64,
    pub tier: u8,
}

/// Fill a circular zone for the trips the apworld assigned (`slot_data.trips`): each gets a point in its tier band.
/// `step_m` is the effective meters per tier (`tier_step_m * (1-p)^reductions`).
#[uniffi::export]
pub fn generate_trips_for(
    center: GeoPoint,
    step_m: f64,
    specs: Vec<TripSpecIn>,
    seed: u64,
    mode: FillMode,
    cache_dir: String,
) -> Result<Vec<TripOut>, CoreError> {
    let zone = Zone::Circle { center: Point::new(center.lat, center.lon), radius_m: step_m * 10.0 };
    let candidates = match mode {
        FillMode::Cells => lattice(&zone, 150.0),
        FillMode::Streets => {
            fetch_streets(&zone, 50.0, Some(&PathBuf::from(cache_dir))).map_err(|e| CoreError::Failed { detail: e.to_string() })?
        }
    };
    let core_specs: Vec<TripSpec> =
        specs.iter().enumerate().map(|(i, s)| TripSpec { number: i as u32 + 1, tier: s.tier }).collect();
    let out = sample(&candidates, zone.home(), &core_specs, step_m, 75.0, seed);
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
            CoreError::Failed { detail } => write!(f, "{detail}"),
        }
    }
}

impl std::error::Error for CoreError {}

#[uniffi::export]
pub fn core_version() -> String {
    format!("apgo-core {}", env!("CARGO_PKG_VERSION"))
}

/// Fill a circular zone around `center` with `trips` trips (tiers cycle 1..=10).
#[uniffi::export]
pub fn generate_trips(
    center: GeoPoint,
    radius_m: f64,
    trips: u32,
    seed: u64,
    mode: FillMode,
    cache_dir: String,
) -> Result<Vec<TripOut>, CoreError> {
    let zone = Zone::Circle { center: Point::new(center.lat, center.lon), radius_m };
    let candidates = match mode {
        FillMode::Cells => lattice(&zone, 150.0),
        FillMode::Streets => {
            fetch_streets(&zone, 50.0, Some(&PathBuf::from(cache_dir))).map_err(|e| CoreError::Failed { detail: e.to_string() })?
        }
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

#[derive(Debug, uniffi::Record)]
pub struct ReceivedItemOut {
    pub index: u32,
    pub item_id: i64,
    pub name: String,
    pub sender: String,
    pub progression: bool,
    pub trap: bool,
}

#[derive(Debug, uniffi::Enum)]
pub enum ApEvent {
    Connected,
    ReceivedItems { from_index: u32 },
    Print { text: String },
    Updated,
    Error { detail: String },
    Other,
}

#[derive(uniffi::Object)]
pub struct ApSession {
    conn: Mutex<ap::Connection>,
}

#[uniffi::export]
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
        let mut conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        conn.update()
            .into_iter()
            .map(|e| match e {
                ap::Event::Connected => ApEvent::Connected,
                ap::Event::ReceivedItems(i) => ApEvent::ReceivedItems { from_index: i as u32 },
                ap::Event::Print(p) => ApEvent::Print { text: p.to_string() },
                ap::Event::Updated(_) => ApEvent::Updated,
                ap::Event::Error(err) => ApEvent::Error { detail: err.to_string() },
                _ => ApEvent::Other,
            })
            .collect()
    }

    /// `connecting`, `connected`, or `disconnected: <reason>`.
    pub fn status(&self) -> String {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        match conn.state() {
            ap::ConnectionState::Connecting(_) => "connecting".into(),
            ap::ConnectionState::Connected(_) => "connected".into(),
            ap::ConnectionState::Disconnected(err) => format!("disconnected: {err}"),
        }
    }

    /// The slot_data the apworld sent (JSON), once connected.
    pub fn slot_data_json(&self) -> Option<String> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        conn.client().map(|c| c.slot_data().to_string())
    }

    pub fn received_items(&self) -> Vec<ReceivedItemOut> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let Some(client) = conn.client() else { return vec![] };
        client
            .received_items()
            .iter()
            .map(|r| {
                let item = r.item();
                ReceivedItemOut {
                    index: r.index() as u32,
                    item_id: item.id(),
                    name: item.name().to_string(),
                    sender: r.sender().name().to_string(),
                    progression: r.is_progression(),
                    trap: r.is_trap(),
                }
            })
            .collect()
    }

    /// Tell the server this location was checked.
    pub fn send_check(&self, location_id: i64) -> Result<(), CoreError> {
        let mut conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let client = conn.client_mut().ok_or_else(|| CoreError::Failed { detail: "not connected".into() })?;
        client.mark_checked([location_id]).map_err(|e| CoreError::Failed { detail: e.to_string() })
    }
}
