//! Pick a real-world candidate for each trip, honoring its distance tier.

use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};

use crate::geo::{distance_m, Point};
use crate::overpass::Candidate;

#[derive(Debug, Clone, Copy)]
pub struct TripSpec {
    pub number: u32,
    pub tier: u8,
}

#[derive(Debug, Clone)]
pub struct Trip {
    pub number: u32,
    pub tier: u8,
    pub candidate: Candidate,
    pub distance_m: f64,
    /// False when no candidate fit the tier band and the nearest one was used.
    pub in_band: bool,
}

/// Tier `t` targets distances in `((t-1)*step, t*step]`. Candidates are never reused and trips stay at
/// least `min_spacing_m` apart; if a band is empty the closest remaining candidate is used.
pub fn sample(
    candidates: &[Candidate],
    home: Point,
    specs: &[TripSpec],
    step_m: f64,
    min_spacing_m: f64,
    seed: u64,
) -> Vec<Trip> {
    let mut rng = StdRng::seed_from_u64(seed);
    let dists: Vec<f64> = candidates.iter().map(|c| distance_m(home, c.point)).collect();
    let mut used = vec![false; candidates.len()];
    let mut chosen: Vec<Point> = Vec::new();
    let mut trips = Vec::new();

    for spec in specs {
        let (lo, hi) = (f64::from(spec.tier - 1) * step_m, f64::from(spec.tier) * step_m);
        let free = |i: usize, used: &[bool], chosen: &[Point]| {
            !used[i] && chosen.iter().all(|p| distance_m(*p, candidates[i].point) >= min_spacing_m)
        };
        let in_band: Vec<usize> = (0..candidates.len())
            .filter(|&i| free(i, &used, &chosen) && dists[i] > lo && dists[i] <= hi)
            .collect();

        let (pick, band) = if in_band.is_empty() {
            let mid = (lo + hi) / 2.0;
            let nearest = (0..candidates.len())
                .filter(|&i| free(i, &used, &chosen))
                .min_by(|&a, &b| (dists[a] - mid).abs().total_cmp(&(dists[b] - mid).abs()));
            match nearest {
                Some(i) => (i, false),
                None => continue,
            }
        } else {
            let total: f64 = in_band.iter().map(|&i| 1.0 + f64::from(candidates[i].score)).sum();
            let mut roll = rng.random_range(0.0..total);
            let mut picked = in_band[in_band.len() - 1];
            for &i in &in_band {
                roll -= 1.0 + f64::from(candidates[i].score);
                if roll < 0.0 {
                    picked = i;
                    break;
                }
            }
            (picked, true)
        };

        used[pick] = true;
        chosen.push(candidates[pick].point);
        trips.push(Trip {
            number: spec.number,
            tier: spec.tier,
            candidate: candidates[pick].clone(),
            distance_m: dists[pick],
            in_band: band,
        });
    }
    trips
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geo::{distance_m, Point};
    use crate::overpass::Candidate;

    fn cand(i: usize, north_m: f64, score: u32) -> Candidate {
        // 1 degree of latitude is ~111_195 m
        Candidate {
            id: format!("n{i}"),
            point: Point::new(40.0 + north_m / 111_195.0, -111.0),
            name: format!("Place {i}"),
            score,
            rough: false,
        }
    }

    fn ring(n: usize) -> Vec<Candidate> {
        // one candidate every 100 m from 100 m to n*100 m north of home
        (1..=n).map(|i| cand(i, i as f64 * 100.0, 1)).collect()
    }

    fn specs(n: u32, tier: u8) -> Vec<TripSpec> {
        (1..=n).map(|number| TripSpec { number, tier }).collect()
    }

    const HOME: Point = Point { lat: 40.0, lon: -111.0 };

    #[test]
    fn trips_land_inside_their_tier_band_when_candidates_exist() {
        let step = 500.0; // tier 3 band: (1000, 1500]
        let trips = sample(&ring(30), HOME, &specs(3, 3), step, 50.0, 1);
        assert_eq!(trips.len(), 3);
        for t in &trips {
            let d = distance_m(HOME, t.candidate.point);
            assert!(d > 1000.0 && d <= 1500.0, "d={d}");
            assert!(t.in_band);
        }
    }

    #[test]
    fn no_candidate_is_used_twice() {
        let trips = sample(&ring(30), HOME, &specs(10, 3), 500.0, 0.0, 1);
        let mut ids: Vec<_> = trips.iter().map(|t| t.candidate.id.clone()).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), trips.len());
    }

    #[test]
    fn same_seed_same_result() {
        let a = sample(&ring(40), HOME, &specs(5, 2), 500.0, 50.0, 9);
        let b = sample(&ring(40), HOME, &specs(5, 2), 500.0, 50.0, 9);
        let ids = |v: &Vec<Trip>| v.iter().map(|t| t.candidate.id.clone()).collect::<Vec<_>>();
        assert_eq!(ids(&a), ids(&b));
    }

    #[test]
    fn empty_band_falls_back_to_nearest_and_says_so() {
        // all candidates are < 300 m; tier 5 band is (2000, 2500]
        let trips = sample(&ring(3), HOME, &specs(1, 5), 500.0, 0.0, 1);
        assert_eq!(trips.len(), 1);
        assert!(!trips[0].in_band);
    }

    #[test]
    fn respects_minimum_spacing_between_trips() {
        let trips = sample(&ring(30), HOME, &specs(5, 3), 500.0, 250.0, 1);
        for (i, a) in trips.iter().enumerate() {
            for b in &trips[i + 1..] {
                assert!(distance_m(a.candidate.point, b.candidate.point) >= 250.0);
            }
        }
    }

    #[test]
    fn no_candidates_means_no_trips() {
        assert!(sample(&[], HOME, &specs(3, 1), 500.0, 0.0, 1).is_empty());
    }

    #[test]
    fn higher_score_is_preferred_overall() {
        let mut c = ring(30);
        c[14].score = 1000; // 1500 m, top of the tier-3 band
        let hits = (0..50u64)
            .filter(|seed| {
                sample(&c, HOME, &specs(1, 3), 500.0, 0.0, *seed)[0].candidate.id == "n15"
            })
            .count();
        assert!(hits > 25, "high score picked only {hits}/50");
    }
}
