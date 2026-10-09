//! Location estimation: one source of truth for where the player is (an IMM Kalman filter), what the map shows (map matching) and how a
//! GPS gap is bridged (steps and heading). Spec: `docs/superpowers/specs/2026-10-08-location-quality-design.md`.

pub mod bench;
pub mod bridge;
pub mod calib;
pub mod frame;
pub mod graph;
pub mod heading;
pub mod imm;
mod locator;
pub mod mat;
pub mod matcher;
pub mod params;

pub use locator::{Locator, Odometer, StepHistory};
pub use params::LocParams;

use serde::{Deserialize, Serialize};

use crate::catalog::Mode;
use crate::geo::Point;
use crate::realm::Shape;

/// An estimate this uncertain (68 % radius, metres) or worse may not complete or advance a quest. The old raw-fix limit, now on the estimate.
pub const MAX_UNCERTAINTY_M: f64 = 35.0;
/// Android's `accuracy` (and iOS `horizontalAccuracy`) is the 68 % radius: for a circular 2-D Gaussian that is 1.515 sigma per axis.
pub const ACC_TO_SIGMA: f64 = 1.515;

/// Where a fix came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    /// Android's fused provider (`HIGH_ACCURACY`: the GNSS chip when it has a fix).
    #[default]
    Fused,
    /// Android's GNSS provider.
    Gps,
    /// Wi-Fi and cell towers.
    Network,
    /// iOS Core Location.
    Ios,
    /// The developer simulator.
    Sim,
    /// Anything else.
    Other,
}

impl Provider {
    /// The provider named `s` as the phone reports it; anything unknown is [`Provider::Other`].
    #[must_use]
    pub fn parse(s: &str) -> Self {
        match s {
            "fused" => Self::Fused,
            "gps" => Self::Gps,
            "network" => Self::Network,
            "ios" => Self::Ios,
            "sim" => Self::Sim,
            _ => Self::Other,
        }
    }

    /// The name [`Self::parse`] reads.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Fused => "fused",
            Self::Gps => "gps",
            Self::Network => "network",
            Self::Ios => "ios",
            Self::Sim => "sim",
            Self::Other => "other",
        }
    }

    /// Whether the fix comes from a satellite receiver.
    #[must_use]
    pub fn is_gnss(self) -> bool {
        matches!(self, Self::Fused | Self::Gps | Self::Ios)
    }
}

/// One position reading as the phone delivered it (`None` = the phone did not report that field).
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct RawFix {
    /// When the fix was taken, Unix ms (the fix's own clock).
    pub t_ms: i64,
    /// Latitude, degrees.
    pub lat: f64,
    /// Longitude, degrees.
    pub lon: f64,
    /// 68 % horizontal radius, metres.
    pub accuracy_m: f64,
    /// Ground speed, m/s.
    pub speed_mps: Option<f64>,
    /// 68 % speed accuracy, m/s.
    pub speed_acc_mps: Option<f64>,
    /// Course over ground, degrees from north.
    pub bearing_deg: Option<f64>,
    /// 68 % bearing accuracy, degrees.
    pub bearing_acc_deg: Option<f64>,
    /// Altitude, metres.
    pub altitude_m: Option<f64>,
    /// 68 % vertical accuracy, metres.
    pub vertical_acc_m: Option<f64>,
    /// Which provider made it.
    pub provider: Provider,
    /// Whether a mock-location app made it.
    pub mock: bool,
}

impl RawFix {
    /// A fused fix with position, time and accuracy only.
    #[must_use]
    pub fn at(lat: f64, lon: f64, t_ms: i64, accuracy_m: f64) -> Self {
        Self { t_ms, lat, lon, accuracy_m, ..Self::default() }
    }

    /// The fix as a map point.
    #[must_use]
    pub fn point(&self) -> Point {
        Point::new(self.lat, self.lon)
    }
}

/// The motion model that explains the player best right now.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Motion {
    /// Standing still.
    #[default]
    Stationary,
    /// Walking or running.
    Walking,
    /// Biking or driving.
    Fast,
}

/// Where an estimate's position comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Source {
    /// A GPS fix through the filter.
    #[default]
    Gps,
    /// Steps and heading on the street graph during a GPS gap.
    Bridged,
    /// The filter's prediction with no new fix.
    Predicted,
}

/// What became of the newest fix.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Verdict {
    /// Used as measured.
    #[default]
    Used,
    /// Used with a down-weighted measurement (it was a bit far from the prediction).
    Soft,
    /// Rejected as a GPS jump.
    Gated,
    /// The estimate is too uncertain to count.
    Blurry,
    /// A real relocation: the filter restarted at the newest fix.
    Relocated,
    /// The filter restarted (first fix, long gap, or lost).
    Reset,
    /// Dropped before the filter (too coarse, out of order, mock, invalid).
    Unusable,
}

impl Verdict {
    /// Whether an estimate with this verdict may count for quests (if it is also sure enough and from GPS).
    #[must_use]
    pub fn may_count(self) -> bool {
        matches!(self, Self::Used | Self::Soft | Self::Relocated | Self::Reset)
    }
}

/// The filter's best estimate of where the player is. Quests, fog, chains, the odometer and the journal use this, never a raw fix.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct Estimate {
    /// Time of the fix (or step batch) it is for, Unix ms.
    pub t_ms: i64,
    /// Latitude, degrees.
    pub lat: f64,
    /// Longitude, degrees.
    pub lon: f64,
    /// 68 % radius: 1.515 * sqrt(largest eigenvalue of the position covariance), metres.
    pub uncertainty_m: f64,
    /// Speed, m/s.
    pub speed_mps: f64,
    /// One sigma of the speed, m/s.
    pub speed_sigma_mps: f64,
    /// Direction of travel, degrees from north, when moving clearly enough to say.
    pub course_deg: Option<f64>,
    /// The most likely motion model.
    pub motion: Motion,
    /// Probabilities of stationary, walking and fast.
    pub mode_probs: [f32; 3],
    /// Where the position comes from.
    pub source: Source,
    /// What became of the fix.
    pub verdict: Verdict,
    /// Whether it may complete or advance a quest.
    pub accepted: bool,
}

impl Estimate {
    /// The estimate as a map point.
    #[must_use]
    pub fn point(&self) -> Point {
        Point::new(self.lat, self.lon)
    }

    /// An accepted, exact (3 m) estimate at a point, standing still: tests and simulated fixes.
    #[must_use]
    pub fn exact(lat: f64, lon: f64, t_ms: i64) -> Self {
        Self { t_ms, lat, lon, uncertainty_m: 3.0, mode_probs: [0.0, 1.0, 0.0], motion: Motion::Walking, accepted: true, ..Self::default() }
    }

    /// Whether only its uncertainty (over 35 m) keeps this GPS estimate from counting: `Blurry`, or a restart (`Reset`, `Relocated`)
    /// that is not accepted (ruling FR-I1).
    #[must_use]
    pub fn uncertain(&self) -> bool {
        self.source == Source::Gps && !self.accepted && matches!(self.verdict, Verdict::Blurry | Verdict::Reset | Verdict::Relocated)
    }
}

/// How sure the phone is of its compass (Android `SENSOR_STATUS_ACCURACY_*`, iOS `headingAccuracy` bands).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum CompassAccuracy {
    /// High.
    High,
    /// Medium.
    Medium,
    /// Low.
    Low,
    /// Unreliable: ignore the reading.
    #[default]
    Unreliable,
}

impl CompassAccuracy {
    /// One sigma of the azimuth, degrees (15 / 30 / 45); `None` when unreliable.
    #[must_use]
    pub fn sigma_deg(self) -> Option<f64> {
        match self {
            Self::High => Some(15.0),
            Self::Medium => Some(30.0),
            Self::Low => Some(45.0),
            Self::Unreliable => None,
        }
    }

    /// `high`, `medium`, `low`; anything else is unreliable.
    #[must_use]
    pub fn parse(s: &str) -> Self {
        match s {
            "high" => Self::High,
            "medium" => Self::Medium,
            "low" => Self::Low,
            _ => Self::Unreliable,
        }
    }
}

/// A heading error of 180 degrees is the phone saying it has no idea of the heading.
const NO_IDEA_ERROR_DEG: f64 = 180.0;
/// No compass sigma is smaller (a reported error of 0 would make a gap's bearing exact).
const MIN_COMPASS_SIGMA_DEG: f64 = 1.0;
/// The sigma of [`CompassAccuracy::Medium`]: the worst a compass may be and still draw the standing arrow.
const ACCURATE_COMPASS_SIGMA_DEG: f64 = 30.0;

/// One compass reading: true-north azimuth of the phone's top edge and how the phone is tilted.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct HeadingIn {
    /// When it was read, Unix ms.
    pub t_ms: i64,
    /// Azimuth, degrees from true north.
    pub azimuth_deg: f64,
    /// Sensor accuracy.
    pub accuracy: CompassAccuracy,
    /// Pitch, degrees.
    pub pitch_deg: f64,
    /// Roll, degrees.
    pub roll_deg: f64,
    /// The phone's own heading error when it gives one (Google's fused orientation `headingErrorDegrees`: half of a 95 % cone, about
    /// two sigma), degrees; when present it replaces [`Self::accuracy`].
    #[serde(default)]
    pub error_deg: Option<f64>,
}

impl HeadingIn {
    /// One sigma of the azimuth, degrees: half of [`Self::error_deg`] when the phone gave one (`None` at 180, "no idea", or a value that
    /// is not one), else the [`CompassAccuracy`] band's.
    #[must_use]
    pub fn sigma_deg(&self) -> Option<f64> {
        match self.error_deg {
            Some(e) if (0.0..NO_IDEA_ERROR_DEG).contains(&e) => Some((e / 2.0).max(MIN_COMPASS_SIGMA_DEG)),
            Some(_) => None,
            None => self.accuracy.sigma_deg(),
        }
    }

    /// Good enough to show as the arrow while standing: a sigma no worse than [`CompassAccuracy::Medium`]'s.
    #[must_use]
    pub fn accurate(&self) -> bool {
        self.sigma_deg().is_some_and(|s| s <= ACCURATE_COMPASS_SIGMA_DEG)
    }
}

/// Where the map arrow's direction comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum HeadingSource {
    /// The direction of travel.
    Course,
    /// The compass (phone held flat and steady; standing, or moving with no course yet).
    Compass,
    /// No arrow.
    #[default]
    None,
}

/// What kind of position the map pin shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum DisplaySource {
    /// From a recent GPS fix.
    #[default]
    Gps,
    /// From steps and heading in a GPS gap (pin drawn hollow).
    Bridged,
    /// Predicted from the last fix.
    Predicted,
    /// Too old to trust (pin greyed).
    Stale,
}

/// What the map shows for the player right now. Display only: quests never see this.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct DisplayPosition {
    /// Shown latitude (matched, bridged or estimated, predicted up to 3 s ahead).
    pub lat: f64,
    /// Shown longitude.
    pub lon: f64,
    /// The estimate's latitude.
    pub est_lat: f64,
    /// The estimate's longitude.
    pub est_lon: f64,
    /// 68 % radius of the shown position, metres.
    pub uncertainty_m: f64,
    /// Speed, m/s.
    pub speed_mps: f64,
    /// Course, degrees.
    pub course_deg: Option<f64>,
    /// Arrow direction, degrees.
    pub heading_deg: Option<f64>,
    /// Where the arrow comes from.
    pub heading_source: HeadingSource,
    /// Whether the pin sits on a matched street.
    pub matched: bool,
    /// Matching confidence, 0..1.
    pub match_confidence: f64,
    /// What kind of position it is.
    pub source: DisplaySource,
    /// Age of the estimate behind it, ms.
    pub age_ms: i64,
    /// True when the newest fix restarted the filter (reset, relocation, simulated fix): jump, do not glide. A level, not an event: it
    /// stays true until the next fix.
    pub snap: bool,
    /// The shown position comes from the gap bridge, also once it ages to "predicted" or "stale" (ruling T22-R3): never a zone.
    #[serde(default)]
    pub bridged_origin: bool,
}

/// The travel mode at `p`: the mode of a zone that contains it (the fastest if several do), else the fastest mode of all `zones`, so a
/// cyclist between zones is never judged as a walker. `None` with no zones.
#[must_use]
pub fn mode_at(zones: &[(Shape, Mode)], p: Point) -> Option<Mode> {
    let inside = zones.iter().filter(|(s, _)| s.distance_m(p) == 0.0).map(|(_, m)| *m).max();
    inside.or_else(|| zones.iter().map(|(_, m)| *m).max())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::realm::Shape;

    #[test]
    fn an_estimate_is_uncertain_when_only_its_uncertainty_keeps_it_from_counting() {
        let e = |verdict, accepted, source| Estimate { verdict, accepted, source, uncertainty_m: 45.0, ..Estimate::default() };
        assert!(e(Verdict::Blurry, false, Source::Gps).uncertain());
        assert!(e(Verdict::Reset, false, Source::Gps).uncertain() && e(Verdict::Relocated, false, Source::Gps).uncertain());
        assert!(!e(Verdict::Reset, true, Source::Gps).uncertain() && !e(Verdict::Gated, false, Source::Gps).uncertain());
        assert!(!e(Verdict::Used, false, Source::Bridged).uncertain() && !e(Verdict::Unusable, false, Source::Gps).uncertain());
    }

    #[test]
    fn only_used_soft_relocated_and_reset_may_count() {
        let yes = [Verdict::Used, Verdict::Soft, Verdict::Relocated, Verdict::Reset];
        let no = [Verdict::Gated, Verdict::Blurry, Verdict::Unusable];
        assert!(yes.iter().all(|v| v.may_count()) && no.iter().all(|v| !v.may_count()));
    }

    #[test]
    fn providers_parse_and_only_satellite_ones_are_gnss() {
        assert_eq!(Provider::parse("gps"), Provider::Gps);
        assert_eq!(Provider::parse("fused"), Provider::Fused);
        assert_eq!(Provider::parse("network"), Provider::Network);
        assert_eq!(Provider::parse("whatever"), Provider::Other);
        assert!(Provider::Fused.is_gnss() && Provider::Gps.is_gnss() && !Provider::Network.is_gnss() && !Provider::Sim.is_gnss());
        assert_eq!(Provider::parse(Provider::Ios.name()), Provider::Ios);
    }

    #[test]
    fn compass_accuracy_maps_to_the_spec_sigmas() {
        assert_eq!(CompassAccuracy::High.sigma_deg(), Some(15.0));
        assert_eq!(CompassAccuracy::Medium.sigma_deg(), Some(30.0));
        assert_eq!(CompassAccuracy::Low.sigma_deg(), Some(45.0));
        assert_eq!(CompassAccuracy::Unreliable.sigma_deg(), None);
        assert_eq!(CompassAccuracy::parse("medium"), CompassAccuracy::Medium);
        assert_eq!(CompassAccuracy::parse("?"), CompassAccuracy::Unreliable);
    }

    #[test]
    fn the_phones_heading_error_sets_the_sigma_when_given() {
        let h = |accuracy, error_deg| HeadingIn { accuracy, error_deg, ..HeadingIn::default() };
        assert_eq!(h(CompassAccuracy::Unreliable, Some(20.0)).sigma_deg(), Some(10.0), "half the 95 % cone, over the band");
        assert_eq!(h(CompassAccuracy::High, Some(100.0)).sigma_deg(), Some(50.0), "the error wins over the band");
        assert_eq!(h(CompassAccuracy::High, Some(0.0)).sigma_deg(), Some(1.0), "never better than one degree");
        for bad in [180.0, 200.0, -1.0, f64::NAN, f64::INFINITY] {
            assert_eq!(h(CompassAccuracy::High, Some(bad)).sigma_deg(), None, "{bad}");
        }
        assert_eq!(h(CompassAccuracy::Medium, None).sigma_deg(), Some(30.0), "no error given: the band");
        assert_eq!(h(CompassAccuracy::Unreliable, None).sigma_deg(), None);
    }

    #[test]
    fn an_exact_estimate_is_accepted_at_three_metres() {
        let e = Estimate::exact(40.0, -111.0, 5_000);
        assert!(e.accepted && e.verdict == Verdict::Used && e.source == Source::Gps);
        assert!((e.uncertainty_m - 3.0).abs() < 1e-9 && e.point() == Point::new(40.0, -111.0));
    }

    #[test]
    fn the_mode_is_the_containing_zones_else_the_fastest_of_the_game() {
        let c = |lat: f64| Shape::Circle { center: Point::new(lat, 0.0), radius_m: 500.0 };
        let zones = [(c(0.0), Mode::Walk), (c(1.0), Mode::Bike)];
        assert_eq!(mode_at(&zones, Point::new(0.0, 0.0)), Some(Mode::Walk));
        assert_eq!(mode_at(&zones, Point::new(1.0, 0.0)), Some(Mode::Bike));
        assert_eq!(mode_at(&zones, Point::new(0.5, 0.0)), Some(Mode::Bike), "between zones: the fastest mode");
        assert_eq!(mode_at(&[], Point::new(0.5, 0.0)), None);
    }
}
