//! Spike B2: fill a zone with trips. Works with streets, an offline lattice, POIs, or a mix.
//! Usage: gen_zone <streets|cells|pois|mixed> <circle lat lon r | annulus lat lon min max | poly "lat,lon;lat,lon;...">
//!        <trips> [seed]

use std::path::PathBuf;
use std::time::Instant;

use apgo_core::fill::{fetch_streets, lattice};
use apgo_core::geo::Point;
use apgo_core::overpass::{fetch_pois, Candidate};
use apgo_core::sampler::{sample, TripSpec};
use apgo_core::zone::Zone;

fn num(s: &str) -> f64 {
    s.parse().unwrap_or_else(|_| panic!("bad number {s}"))
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let usage = "gen_zone <streets|cells|pois|mixed> <circle lat lon r | annulus lat lon min max | poly \"lat,lon;...\"> <trips> [seed]";
    let mode = a.get(1).expect(usage).as_str();
    let (zone, rest) = match a.get(2).expect(usage).as_str() {
        "circle" => (Zone::Circle { center: Point::new(num(&a[3]), num(&a[4])), radius_m: num(&a[5]) }, 6),
        "annulus" => (Zone::Annulus { center: Point::new(num(&a[3]), num(&a[4])), min_m: num(&a[5]), max_m: num(&a[6]) }, 7),
        "poly" => {
            let pts = a[3]
                .split(';')
                .map(|p| {
                    let (la, lo) = p.split_once(',').expect("lat,lon");
                    Point::new(num(la), num(lo))
                })
                .collect();
            (Zone::Polygon(pts), 4)
        }
        other => panic!("unknown zone {other}; {usage}"),
    };
    let trips: u32 = a[rest].parse().expect("trips");
    let seed: u64 = a.get(rest + 1).map_or(1, |s| s.parse().expect("seed"));
    let cache = PathBuf::from(std::env::var("HOME").unwrap_or_default()).join(".cache/apgo-spike");

    let t0 = Instant::now();
    let mut cands: Vec<Candidate> = Vec::new();
    if matches!(mode, "streets" | "mixed") {
        cands.extend(fetch_streets(&zone, 50.0, Some(&cache)).unwrap_or_else(|e| {
            eprintln!("streets: {e}");
            vec![]
        }));
    }
    if matches!(mode, "pois" | "mixed") {
        let r = zone.max_extent_m() as u32;
        cands.extend(
            fetch_pois(zone.home(), r, Some(&cache))
                .unwrap_or_else(|e| {
                    eprintln!("pois: {e}");
                    vec![]
                })
                .into_iter()
                .filter(|c| zone.contains(c.point))
                .map(|mut c| {
                    c.score += 5;
                    c
                }),
        );
    }
    if mode == "cells" {
        cands = lattice(&zone, 150.0);
    }
    let gather_ms = t0.elapsed().as_millis();

    let specs: Vec<TripSpec> = (1..=trips).map(|n| TripSpec { number: n, tier: ((n - 1) % 10) as u8 + 1 }).collect();
    let step = zone.max_extent_m() / 10.0;
    let t1 = Instant::now();
    let out = sample(&cands, zone.home(), &specs, step, 75.0, seed);
    let us = t1.elapsed().as_micros();
    for t in out.iter().take(8) {
        println!("#{:<4} tier {:<2} {:>7.0} m  {}{}", t.number, t.tier, t.distance_m, t.candidate.name, if t.in_band { "" } else { "  (out of band)" });
    }
    let in_band = out.iter().filter(|t| t.in_band).count();
    let outside = out.iter().filter(|t| !zone.contains(t.candidate.point)).count();
    println!(
        "--\nmode {mode}  candidates {}  trips {}/{}  in-band {}  outside-zone {}  gather {} ms  sample {} us",
        cands.len(),
        out.len(),
        trips,
        in_band,
        outside,
        gather_ms,
        us
    );
}
