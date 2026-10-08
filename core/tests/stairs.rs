//! Regression test: a real staircase from OpenStreetMap data can be completed by walking it.

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
        p(45.516_151_4, -122.694_954),
        p(45.516_200_2, -122.694_921_7),
        p(45.516_328_5, -122.694_859_6),
        p(45.516_528, -122.694_710_8),
        p(45.516_596, -122.694_700_8),
        p(45.516_649_1, -122.694_743_1),
        p(45.516_655_7, -122.694_73),
        p(45.516_634_9, -122.694_697_7),
        p(45.516_641_6, -122.694_686_2),
        p(45.516_678_1, -122.694_707_9),
        p(45.516_693_5, -122.694_686_5),
    ];
    let mut t = Tracker::new(Target::Line { pts: pts.clone(), corridor_m: 15.0, coverage: 0.8 }, pts[0]);
    let mut ms = 0;
    let mut status = Status::Idle;
    for w in pts.windows(2) {
        for k in 1..=3 {
            let f = f64::from(k) / 3.0;
            ms += 15_000;
            status = t.update(&Fix { lat: w[0].lat + (w[1].lat - w[0].lat) * f, lon: w[0].lon + (w[1].lon - w[0].lon) * f, t_ms: ms, accuracy_m: 5.0 }, None);
        }
    }
    assert_eq!(status, Status::Done, "walking the whole staircase must complete it");
}
