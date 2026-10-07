use apgo_core::assign::Target;
use apgo_core::geo::Point;
use apgo_core::verify::{Fix, Status, Tracker};

fn p(lat: f64, lon: f64) -> Point {
    Point::new(lat, lon)
}

/// Regression: a tiny zig-zag staircase (60 m) from real OSM data must be completable by walking it.
#[test]
fn a_short_real_staircase_can_be_completed_by_walking_it() {
    let pts = vec![
        p(45.5161514, -122.694954),
        p(45.5162002, -122.6949217),
        p(45.5163285, -122.6948596),
        p(45.516528, -122.6947108),
        p(45.516596, -122.6947008),
        p(45.5166491, -122.6947431),
        p(45.5166557, -122.69473),
        p(45.5166349, -122.6946977),
        p(45.5166416, -122.6946862),
        p(45.5166781, -122.6947079),
        p(45.5166935, -122.6946865),
    ];
    let mut t = Tracker::new(Target::Line { pts: pts.clone(), corridor_m: 15.0, coverage: 0.8 }, pts[0]);
    let mut ms = 0;
    let mut status = Status::Idle;
    for w in pts.windows(2) {
        for k in 1..=3 {
            let f = k as f64 / 3.0;
            ms += 15_000;
            status = t.update(&Fix { lat: w[0].lat + (w[1].lat - w[0].lat) * f, lon: w[0].lon + (w[1].lon - w[0].lon) * f, t_ms: ms, accuracy_m: 5.0 }, None);
        }
    }
    assert_eq!(status, Status::Done, "walking the whole staircase must complete it");
}
