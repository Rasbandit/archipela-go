//! UniFFI surface of apgo-core. Blocking calls: invoke from a background thread on the host side.

use std::path::PathBuf;

use apgo_core::fill::{fetch_streets, lattice};
use apgo_core::geo::Point;
use apgo_core::sampler::{sample, TripSpec};
use apgo_core::zone::Zone;

uniffi::setup_scaffolding!();

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
            tier: t.tier,
            lat: t.candidate.point.lat,
            lon: t.candidate.point.lon,
            name: t.candidate.name,
            distance_m: t.distance_m,
            in_band: t.in_band,
        })
        .collect())
}
