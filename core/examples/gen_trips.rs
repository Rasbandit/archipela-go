//! Spike B demo: fetch POIs in one bulk query, sample trips, print timings.
//! Usage: cargo run --release --example gen_trips -- <lat> <lon> <max_m> <trips> [seed]

use std::path::PathBuf;
use std::time::Instant;

use apgo_core::geo::Point;
use apgo_core::overpass::fetch_pois;
use apgo_core::sampler::{sample, TripSpec};

fn main() {
    let a: Vec<String> = std::env::args().collect();
    if a.len() < 5 {
        eprintln!("usage: gen_trips <lat> <lon> <max_m> <trips> [seed]");
        std::process::exit(2);
    }
    let home = Point::new(a[1].parse().expect("lat"), a[2].parse().expect("lon"));
    let max_m: f64 = a[3].parse().expect("max_m");
    let trips: u32 = a[4].parse().expect("trips");
    let seed: u64 = a.get(5).map_or(1, |s| s.parse().expect("seed"));

    let cache = PathBuf::from(std::env::var("XDG_CACHE_HOME").unwrap_or_else(|_| format!("{}/.cache", std::env::var("HOME").unwrap_or_default()))).join("apgo-spike");
    let t0 = Instant::now();
    let candidates = fetch_pois(home, max_m as u32, Some(&cache)).unwrap_or_else(|e| {
        eprintln!("fetch failed: {e}");
        std::process::exit(1);
    });
    let fetch_ms = t0.elapsed().as_millis();

    let specs: Vec<TripSpec> = (1..=trips).map(|n| TripSpec { number: n, tier: ((n - 1) % 10) as u8 + 1 }).collect();
    let t1 = Instant::now();
    let out = sample(&candidates, home, &specs, max_m / 10.0, 75.0, seed);
    let sample_us = t1.elapsed().as_micros();

    for t in &out {
        println!("#{:<4} tier {:<2} {:>7.0} m  {}{}", t.number, t.tier, t.distance_m, t.candidate.name, if t.in_band { "" } else { "  (out of band)" });
    }
    let in_band = out.iter().filter(|t| t.in_band).count();
    println!("--\ncandidates {}  trips {}/{}  in-band {}  fetch {} ms  sample {} us", candidates.len(), out.len(), trips, in_band, fetch_ms, sample_us);
}
