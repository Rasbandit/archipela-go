//! Whether the player is at a camp (home is one): a camp's Wi-Fi or a camp circle, and when to look with GPS to tell.
//!
//! Camp Wi-Fi alone left on every Wi-Fi drop (some phones drop it for minutes), turning GPS on and drawing points at camp. With camps
//! pinned, leaving needs a fix outside every circle. GPS stays off at home and is asked for only on an event: a walk starting
//! (a burst of steps) or home Wi-Fi dropping. Fixes inside the circle never reach the game, the journal or the trace.

use std::collections::VecDeque;

use crate::geo::{distance_m, Point};

/// Radius of a camp circle, metres: it covers how far GPS wanders at a camp (indoors 10-30 m).
pub const CAMP_CIRCLE_M: f64 = 50.0;
/// A walk starts with this many steps ...
pub const WALK_BURST_STEPS: i64 = 30;
/// ... within this long, ms (pacing a room rarely keeps it up).
pub const WALK_BURST_MS: i64 = 30_000;
/// A check gives GPS this long to tell inside from outside, ms; then it gives up and falls back.
pub const CHECK_MS: i64 = 60_000;
/// After a check found the player home, steps start no new check for this long, ms (walking about the house).
pub const RECHECK_AFTER_MS: i64 = 120_000;
/// Fixes in a row inside the circle that bring a player home.
pub const ARRIVE_FIXES: u8 = 2;

/// Where the player is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HomeState {
    /// At a camp: GPS off, nothing counts.
    Home,
    /// Maybe leaving: GPS on to tell, nothing counts yet.
    Checking,
    /// Away from every camp.
    Away,
}

/// A running check: GPS is on until `until_ms`, then the state falls back to `fallback`. `from_ms` is when leaving would have begun.
#[derive(Debug, Clone, Copy)]
struct Check {
    until_ms: i64,
    fallback: HomeState,
    from_ms: i64,
    /// The step total at `from_ms`, when a walk burst started the check.
    from_steps: Option<i64>,
}

/// Camp Wi-Fi and the camp circles, decided from events (no timer but the one check deadline, [`HomeWatch::due_ms`]).
#[derive(Debug, Clone)]
pub struct HomeWatch {
    /// Every camp (id, centre).
    camps: Vec<(String, Point)>,
    /// Whether the phone reports steps: without them GPS-off at home could never see a walk out, so the circle is not used.
    walk_sensor: bool,
    state: HomeState,
    started: bool,
    wifi: Option<bool>,
    /// The camp whose Wi-Fi the phone is on, when the host said which.
    wifi_camp: Option<String>,
    /// The camp the player is at while [`HomeState::Home`] (kept through a check that may fall back home).
    camp: Option<String>,
    /// The newest valid fix, to name the camp when only Wi-Fi says home.
    last_fix: Option<Point>,
    /// Step readings (time, total) of the last [`WALK_BURST_MS`] while home.
    steps: VecDeque<(i64, i64)>,
    check: Option<Check>,
    /// No walk burst starts a check before this (a check just found the player home).
    quiet_until_ms: i64,
    inside_fixes: u8,
    left_at_ms: Option<i64>,
    /// The step total where the walk out began, when a walk burst started the check that left.
    left_steps: Option<i64>,
    /// The time of the newest event seen, for a change that comes with none (the circle going away).
    last_t_ms: i64,
}

impl Default for HomeWatch {
    fn default() -> Self {
        Self {
            camps: Vec::new(),
            walk_sensor: false,
            state: HomeState::Away,
            started: false,
            wifi: None,
            wifi_camp: None,
            camp: None,
            last_fix: None,
            steps: VecDeque::new(),
            check: None,
            quiet_until_ms: i64::MIN,
            inside_fixes: 0,
            left_at_ms: None,
            left_steps: None,
            last_t_ms: 0,
        }
    }
}

impl HomeWatch {
    /// Every camp (id, centre); none leaves it to Wi-Fi alone, at once. A camp added or removed takes effect at once.
    pub fn set_camps(&mut self, camps: Vec<(String, Point)>) {
        self.camps = camps;
        if self.wifi_camp.as_ref().is_some_and(|id| !self.has_camp(id)) {
            self.wifi_camp = None;
        }
        if self.camp.as_ref().is_some_and(|id| !self.has_camp(id)) {
            self.camp = None;
            self.name_camp();
        }
        self.circle_changed();
    }

    /// Whether the phone reports steps (a step counter, and permission to read it); the circle needs them.
    pub fn set_walk_sensor(&mut self, on: bool) {
        self.walk_sensor = on;
        self.circle_changed();
    }

    /// The centres of the circles in use (radius [`CAMP_CIRCLE_M`]): the camps, only with steps to see a walk out.
    #[must_use]
    pub fn circles(&self) -> Vec<Point> {
        if self.walk_sensor {
            self.camps.iter().map(|(_, p)| *p).collect()
        } else {
            Vec::new()
        }
    }

    /// The id of the camp the player is at, while [`HomeState::Home`].
    #[must_use]
    pub fn camp(&self) -> Option<&str> {
        self.camp.as_deref().filter(|_| self.state == HomeState::Home)
    }

    fn has_camp(&self, id: &str) -> bool {
        self.camps.iter().any(|(c, _)| c == id)
    }

    /// The camp nearest `p`, with its distance.
    fn nearest(&self, p: Point) -> Option<(&str, f64)> {
        self.camps.iter().map(|(id, c)| (id.as_str(), distance_m(p, *c))).min_by(|a, b| a.1.total_cmp(&b.1))
    }

    // Names the camp when the state is home without a fix to say which: the Wi-Fi's camp, else the one nearest the last fix.
    fn name_camp(&mut self) {
        let near = self.last_fix.and_then(|p| self.nearest(p)).filter(|&(_, d)| d <= CAMP_CIRCLE_M).map(|(id, _)| id.to_owned());
        let id = self.wifi_camp.clone().or(near);
        self.camp = id;
    }

    // No circle any more: a running check, or a home only the circle kept, ends as Wi-Fi alone would have it.
    fn circle_changed(&mut self) {
        if !self.circles().is_empty() || !self.started || self.state == HomeState::Away {
            return;
        }
        if self.wifi == Some(true) {
            self.go_home(i64::MIN);
            self.name_camp();
        } else {
            let from = self.check.filter(|_| self.state == HomeState::Checking).map_or(self.last_t_ms, |c| c.from_ms);
            self.go_away(from, None);
        }
    }

    /// Start deciding at `t_ms` (presence started): home on home Wi-Fi; with a pin and no home Wi-Fi, a check, falling back to away
    /// (as before the circle: no home Wi-Fi means playing unless a fix shows the player home). A restart keeps a home the circle
    /// decided: a check indoors often finds no clear fix and would fall back to away.
    pub fn start(&mut self, t_ms: i64) {
        self.last_t_ms = t_ms;
        let circle_home = self.started && self.state == HomeState::Home && self.has_camps();
        self.started = true;
        if circle_home {
            return;
        }
        if self.wifi == Some(true) {
            self.go_home(i64::MIN);
            self.name_camp();
        } else if self.has_camps() {
            self.begin_check(t_ms, HomeState::Away, t_ms, None);
        } else {
            self.go_away(t_ms, None);
        }
    }

    /// The (debounced) home Wi-Fi reading: `Some(true)` on it, `Some(false)` on another network, `None` unknown or off. Joining it is
    /// home; losing it at home leaves at once without a pin, and only checks with one. Only a change counts (the host may repeat it).
    pub fn on_wifi(&mut self, home: Option<bool>, t_ms: i64) {
        self.wifi_camp = None;
        self.apply_wifi(home, t_ms);
    }

    /// Camp Wi-Fi: `Some(id)` is the Wi-Fi of that camp (so the player is at a camp, that one), `None` is no camp's Wi-Fi. Joining
    /// another camp's Wi-Fi while at a camp renames the camp.
    pub fn on_wifi_camp(&mut self, camp: Option<String>, t_ms: i64) {
        let given = camp.is_some();
        let known = camp.filter(|id| self.has_camp(id)); // an id we do not know is plain Wi-Fi
        self.wifi_camp.clone_from(&known);
        self.apply_wifi(Some(given), t_ms);
        if known.is_some() {
            self.camp = known; // `camp()` hides it outside home; a check that falls back home keeps it
        }
    }

    fn apply_wifi(&mut self, home: Option<bool>, t_ms: i64) {
        let changed = self.wifi != home;
        self.wifi = home;
        self.last_t_ms = t_ms;
        if !self.started || !changed {
            return;
        }
        if home == Some(true) {
            self.go_home(i64::MIN);
            self.name_camp();
        } else if self.state == HomeState::Home {
            if self.has_camps() {
                self.begin_check(t_ms, HomeState::Home, t_ms, None);
            } else {
                self.go_away(t_ms, None);
            }
        }
    }

    /// A step-counter total (cumulative) at the sensor event's time. At home, [`WALK_BURST_STEPS`] within [`WALK_BURST_MS`] start a
    /// check (unless one just found the player home).
    pub fn on_steps(&mut self, total: i64, t_ms: i64) {
        self.last_t_ms = t_ms;
        if self.state != HomeState::Home || !self.has_camps() {
            return;
        }
        if self.steps.back().is_some_and(|&(t, _)| t_ms - t > WALK_BURST_MS || t > t_ms) {
            self.steps.clear(); // a long quiet spell (or a clock step): the new reading is the baseline
        }
        self.steps.push_back((t_ms, total));
        while self.steps.front().is_some_and(|&(t, _)| t < t_ms - WALK_BURST_MS) {
            self.steps.pop_front();
        }
        let Some(&(first_ms, base)) = self.steps.front() else { return };
        if total - base >= WALK_BURST_STEPS && t_ms >= self.quiet_until_ms {
            self.begin_check(t_ms, HomeState::Home, first_ms, Some(base));
        }
    }

    /// A real fix (never simulated): `accuracy_m` is its 68 % radius. Judged against the nearest camp: only a fix clearly outside every
    /// circle (by more than its accuracy) leaves, and only one clearly inside a circle counts as inside.
    pub fn on_fix(&mut self, p: Point, accuracy_m: f64, t_ms: i64) {
        self.last_t_ms = t_ms;
        if !self.started || !self.has_camps() {
            return;
        }
        if !(p.lat.is_finite() && p.lon.is_finite() && accuracy_m.is_finite() && accuracy_m >= 0.0) {
            return;
        }
        self.last_fix = Some(p);
        let Some((id, d)) = self.nearest(p).map(|(id, d)| (id.to_owned(), d)) else { return };
        let (outside, inside) = (d - accuracy_m > CAMP_CIRCLE_M, d + accuracy_m <= CAMP_CIRCLE_M);
        match self.state {
            HomeState::Checking if outside => {
                let (from, steps) = self.check.map_or((t_ms, None), |c| (c.from_ms, c.from_steps));
                self.go_away(from, steps);
            }
            HomeState::Checking if inside => {
                self.go_home(t_ms + RECHECK_AFTER_MS);
                self.camp = Some(self.wifi_camp.clone().unwrap_or(id));
            }
            HomeState::Home if outside && self.wifi != Some(true) => self.go_away(t_ms, None),
            HomeState::Away if inside => {
                self.inside_fixes += 1;
                if self.inside_fixes >= ARRIVE_FIXES {
                    self.go_home(t_ms + RECHECK_AFTER_MS);
                    self.camp = Some(self.wifi_camp.clone().unwrap_or(id));
                }
            }
            HomeState::Away => self.inside_fixes = 0,
            _ => {}
        }
    }

    /// The check deadline passed by `t_ms`: fall back (home: GPS off again; away: as Wi-Fi alone would have it).
    pub fn tick(&mut self, t_ms: i64) {
        self.last_t_ms = t_ms;
        let Some(c) = self.check.filter(|c| self.state == HomeState::Checking && t_ms >= c.until_ms) else { return };
        if c.fallback == HomeState::Home {
            self.go_home(t_ms + RECHECK_AFTER_MS);
        } else {
            self.go_away(c.from_ms, None);
        }
    }

    /// Whether the camp circles are in use: a camp and a step counter (else camp Wi-Fi alone decides).
    #[must_use]
    pub fn has_camps(&self) -> bool {
        self.walk_sensor && !self.camps.is_empty()
    }

    /// Where the player is.
    #[must_use]
    pub fn state(&self) -> HomeState {
        self.state
    }

    /// When the running check gives up: the host schedules one wake-up then and calls [`Self::tick`].
    #[must_use]
    pub fn due_ms(&self) -> Option<i64> {
        self.check.filter(|_| self.state == HomeState::Checking).map(|c| c.until_ms)
    }

    /// When the walk that left home began (the first step of its burst, or the check's start), while away.
    #[must_use]
    pub fn left_at_ms(&self) -> Option<i64> {
        self.left_at_ms.filter(|_| self.state == HomeState::Away)
    }

    /// Where the walk out began as (step total, time), while away after a check a walk burst started: the steps to credit from.
    #[must_use]
    pub fn left_steps(&self) -> Option<(i64, i64)> {
        self.left_steps.zip(self.left_at_ms).filter(|_| self.state == HomeState::Away)
    }

    fn begin_check(&mut self, t_ms: i64, fallback: HomeState, from_ms: i64, from_steps: Option<i64>) {
        self.state = HomeState::Checking;
        self.check = Some(Check { until_ms: t_ms + CHECK_MS, fallback, from_ms, from_steps });
        self.steps.clear();
    }

    fn go_home(&mut self, quiet_until_ms: i64) {
        self.state = HomeState::Home;
        self.check = None;
        self.steps.clear();
        self.inside_fixes = 0;
        self.left_at_ms = None;
        self.left_steps = None;
        self.quiet_until_ms = quiet_until_ms;
    }

    fn go_away(&mut self, left_at_ms: i64, left_steps: Option<i64>) {
        self.state = HomeState::Away;
        self.check = None;
        self.steps.clear();
        self.inside_fixes = 0;
        self.left_at_ms = Some(left_at_ms);
        self.left_steps = left_steps;
        self.camp = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geo::destination;

    fn home() -> Point {
        Point::new(40.0, -111.0)
    }

    fn out(m: f64) -> Point {
        destination(home(), 90.0, m)
    }

    fn at_home() -> HomeWatch {
        let mut w = HomeWatch::default();
        w.set_walk_sensor(true);
        w.set_camps(vec![("home".into(), home())]);
        w.on_wifi(Some(true), 0);
        w.start(0);
        w
    }

    /// Steps at 2 a second from `from_ms` for `secs` seconds, starting at `total`; returns the total after.
    fn walk(w: &mut HomeWatch, total: i64, from_ms: i64, secs: i64) -> i64 {
        for s in 1..=secs {
            w.on_steps(total + 2 * s, from_ms + s * 1000);
        }
        total + 2 * secs
    }

    #[test]
    fn without_a_step_counter_the_pin_is_ignored_and_wifi_alone_decides() {
        // GPS is off at home, so only steps can tell a walk out: without them a player in the circle would stay paused for good.
        let mut w = HomeWatch::default();
        w.set_camps(vec![("home".into(), home())]);
        w.on_wifi(Some(true), 0);
        w.start(0);
        assert!(!w.has_camps());
        w.on_wifi(None, 1_000);
        assert_eq!(w.state(), HomeState::Away, "a drop leaves, as before the circle");
        w.set_walk_sensor(true);
        w.on_wifi(Some(true), 2_000);
        w.on_wifi(None, 3_000);
        assert_eq!(w.state(), HomeState::Checking, "with steps the circle applies");
    }

    #[test]
    fn home_wifi_is_home_with_or_without_a_pin() {
        assert_eq!(at_home().state(), HomeState::Home);
        let mut no_pin = HomeWatch::default();
        no_pin.on_wifi(Some(true), 0);
        no_pin.start(0);
        assert_eq!(no_pin.state(), HomeState::Home);
    }

    #[test]
    fn without_a_pin_a_wifi_drop_leaves_as_before() {
        let mut w = HomeWatch::default();
        w.on_wifi(Some(true), 0);
        w.start(0);
        w.on_wifi(None, 1000);
        assert_eq!(w.state(), HomeState::Away);
    }

    #[test]
    fn with_a_pin_a_wifi_drop_only_checks_and_a_fix_inside_keeps_home() {
        // The owner's phone drops home Wi-Fi for minutes: that alone must not leave home or draw points there.
        let mut w = at_home();
        w.on_wifi(None, 10_000);
        assert_eq!(w.state(), HomeState::Checking);
        assert_eq!(w.due_ms(), Some(10_000 + CHECK_MS));
        w.on_fix(out(20.0), 25.0, 15_000); // indoors: coarse, but within the circle either way (20 + 25 <= 50)
        assert_eq!(w.state(), HomeState::Home);
        assert_eq!(w.due_ms(), None, "GPS goes off again");
    }

    #[test]
    fn the_same_wifi_reading_again_starts_no_new_check() {
        // The host passes the reading on every evaluation: only a change counts, or a check that found home would restart at once.
        let mut w = at_home();
        w.on_wifi(None, 0);
        w.on_fix(out(5.0), 5.0, 1_000);
        assert_eq!(w.state(), HomeState::Home);
        w.on_wifi(None, 2_000);
        assert_eq!(w.state(), HomeState::Home);
    }

    #[test]
    fn an_unclear_fix_keeps_checking_and_the_deadline_falls_back_home() {
        let mut w = at_home();
        w.on_wifi(None, 0);
        w.on_fix(out(40.0), 30.0, 5_000); // could be either side
        assert_eq!(w.state(), HomeState::Checking);
        w.tick(CHECK_MS - 1);
        assert_eq!(w.state(), HomeState::Checking, "not due yet");
        w.tick(CHECK_MS);
        assert_eq!(w.state(), HomeState::Home, "no clear fix: stay home, GPS off");
    }

    #[test]
    fn a_walk_burst_checks_and_a_fix_outside_leaves_from_the_first_step() {
        let mut w = at_home();
        let total = walk(&mut w, 1000, 100_000, 10); // 20 steps: not yet a walk
        assert_eq!(w.state(), HomeState::Home);
        walk(&mut w, total, 110_000, 6); // 32 steps in 16 s
        assert_eq!(w.state(), HomeState::Checking);
        w.on_fix(out(45.0), 5.0, 130_000); // 45 m out, 5 m accuracy: 40 m is not past 50 m
        assert_eq!(w.state(), HomeState::Checking);
        w.on_fix(out(60.0), 5.0, 140_000);
        assert_eq!(w.state(), HomeState::Away);
        assert_eq!(w.left_at_ms(), Some(101_000), "the walk began with the burst's first step");
        assert_eq!(w.left_steps(), Some((1002, 101_000)), "and the step total there, so only the walk's steps count");
    }

    #[test]
    fn steps_spread_thin_are_no_walk() {
        let mut w = at_home();
        for s in 1..=40 {
            w.on_steps(1000 + s, i64::from(u16::try_from(s).unwrap_or(0)) * 2_000);
            // 1 step every 2 s: 15 in any 30 s
        }
        assert_eq!(w.state(), HomeState::Home);
    }

    #[test]
    fn after_a_check_found_home_walking_about_the_house_waits_before_checking_again() {
        let mut w = at_home();
        let total = walk(&mut w, 0, 0, 16);
        assert_eq!(w.state(), HomeState::Checking);
        w.on_fix(out(10.0), 8.0, 17_000);
        assert_eq!(w.state(), HomeState::Home);
        let total = walk(&mut w, total, 20_000, 20);
        assert_eq!(w.state(), HomeState::Home, "within {RECHECK_AFTER_MS} ms of the last check");
        walk(&mut w, total, 17_000 + RECHECK_AFTER_MS, 16);
        assert_eq!(w.state(), HomeState::Checking, "a walk after the quiet time checks again");
    }

    #[test]
    fn away_two_fixes_inside_bring_the_player_home_and_one_does_not() {
        let mut w = at_home();
        w.on_wifi(None, 0);
        w.on_fix(out(200.0), 5.0, 1_000);
        assert_eq!(w.state(), HomeState::Away);
        w.on_fix(out(30.0), 5.0, 600_000);
        w.on_fix(out(80.0), 5.0, 605_000);
        w.on_fix(out(30.0), 5.0, 610_000);
        assert_eq!(w.state(), HomeState::Away, "one fix inside is not arriving");
        w.on_fix(out(25.0), 5.0, 615_000);
        assert_eq!(w.state(), HomeState::Home);
        assert_eq!(w.due_ms(), None);
    }

    #[test]
    fn away_a_wifi_drop_or_steps_change_nothing_and_home_wifi_arrives() {
        let mut w = at_home();
        w.on_wifi(None, 0);
        w.on_fix(out(200.0), 5.0, 1_000);
        walk(&mut w, 0, 2_000, 20);
        w.on_wifi(None, 30_000);
        assert_eq!(w.state(), HomeState::Away);
        w.on_wifi(Some(true), 40_000);
        assert_eq!(w.state(), HomeState::Home);
    }

    #[test]
    fn starting_with_a_pin_and_no_home_wifi_checks_and_falls_back_to_away() {
        // As before the circle: no home Wi-Fi at the start means playing, unless a fix shows the player home.
        let mut w = HomeWatch::default();
        w.set_walk_sensor(true);
        w.set_camps(vec![("home".into(), home())]);
        w.start(0);
        assert_eq!(w.state(), HomeState::Checking);
        w.tick(CHECK_MS);
        assert_eq!(w.state(), HomeState::Away);
        let mut inside = HomeWatch::default();
        inside.set_walk_sensor(true);
        inside.set_camps(vec![("home".into(), home())]);
        inside.start(0);
        inside.on_fix(out(5.0), 10.0, 2_000);
        assert_eq!(inside.state(), HomeState::Home);
    }

    #[test]
    fn a_restart_keeps_a_home_the_circle_decided() {
        // The host restarts presence (e.g. the Bluetooth grant changed): a player the circle found home, with no home Wi-Fi, must not
        // be put through a check that falls back to away indoors, where GPS often finds no clear fix.
        let mut w = HomeWatch::default();
        w.set_walk_sensor(true);
        w.set_camps(vec![("home".into(), home())]);
        w.start(0);
        w.on_fix(out(5.0), 10.0, 2_000);
        assert_eq!(w.state(), HomeState::Home);
        w.start(10_000);
        assert_eq!(w.state(), HomeState::Home);
        assert_eq!(w.due_ms(), None, "no check, GPS stays off");
    }

    #[test]
    fn losing_the_circle_while_it_alone_keeps_the_player_home_leaves() {
        // No home Wi-Fi: only the circle said home. Without it (step permission revoked, pin cleared) GPS would stay off for good.
        let mut w = HomeWatch::default();
        w.set_walk_sensor(true);
        w.set_camps(vec![("home".into(), home())]);
        w.start(0);
        w.on_fix(out(5.0), 10.0, 2_000);
        assert_eq!(w.state(), HomeState::Home);
        w.set_walk_sensor(false);
        assert_eq!(w.state(), HomeState::Away);
        assert_eq!(w.left_at_ms(), Some(2_000), "from the last thing the watch saw");
        let mut wifi = at_home();
        wifi.set_camps(vec![]);
        assert_eq!(wifi.state(), HomeState::Home, "home Wi-Fi still says home");
    }

    #[test]
    fn a_new_pin_or_none_takes_effect_at_once() {
        let mut w = at_home();
        w.on_wifi(None, 0);
        assert_eq!(w.state(), HomeState::Checking);
        w.set_camps(vec![]);
        assert_eq!(w.state(), HomeState::Away, "no pin: Wi-Fi alone decides, and it dropped");
        assert_eq!(w.due_ms(), None);
    }

    #[test]
    fn a_coarse_or_broken_fix_decides_nothing() {
        let mut w = at_home();
        w.on_wifi(None, 0);
        w.on_fix(out(500.0), f64::NAN, 1_000);
        w.on_fix(Point::new(f64::NAN, 0.0), 5.0, 2_000);
        assert_eq!(w.state(), HomeState::Checking);
    }

    fn camp_b() -> Point {
        destination(home(), 90.0, 80.0) // overlaps home's circle: 80 m apart, 50 m each
    }

    fn two_camps() -> HomeWatch {
        let mut w = HomeWatch::default();
        w.set_walk_sensor(true);
        w.set_camps(vec![("a".into(), home()), ("b".into(), camp_b())]);
        w.start(0);
        w
    }

    #[test]
    fn arriving_at_a_camp_names_the_nearest_one() {
        let mut w = two_camps();
        w.on_fix(out(200.0), 5.0, 1_000);
        assert_eq!((w.state(), w.camp()), (HomeState::Away, None));
        w.on_fix(destination(camp_b(), 90.0, 5.0), 5.0, 600_000);
        w.on_fix(destination(camp_b(), 90.0, 6.0), 5.0, 605_000);
        assert_eq!((w.state(), w.camp()), (HomeState::Home, Some("b")));
    }

    #[test]
    fn leaving_needs_a_fix_outside_both_overlapping_circles() {
        let mut w = two_camps();
        w.on_fix(out(5.0), 5.0, 1_000);
        w.on_fix(out(5.0), 5.0, 2_000);
        assert_eq!(w.camp(), Some("a"));
        w.on_wifi(None, 3_000); // not on Wi-Fi: only a fix decides
        w.on_fix(out(110.0), 5.0, 4_000); // 110 m from a, 30 m from b: inside b's circle
        assert_ne!(w.state(), HomeState::Away);
        w.on_fix(out(150.0), 5.0, 5_000); // 70 m from b, past both circles
        assert_eq!((w.state(), w.camp()), (HomeState::Away, None));
    }

    #[test]
    fn no_camps_with_no_wifi_goes_away() {
        let mut w = at_home();
        w.on_wifi(None, 0);
        assert_eq!(w.state(), HomeState::Checking);
        w.set_camps(vec![]);
        assert_eq!(w.state(), HomeState::Away);
        assert!(!w.has_camps() && w.circles().is_empty());
    }

    #[test]
    fn a_camp_wifi_is_home_at_that_camp() {
        let mut w = two_camps();
        w.on_wifi_camp(Some("a".into()), 1_000);
        assert_eq!((w.state(), w.camp()), (HomeState::Home, Some("a")));
        w.on_wifi_camp(Some("b".into()), 2_000);
        assert_eq!((w.state(), w.camp()), (HomeState::Home, Some("b")), "the name follows the Wi-Fi");
        w.on_wifi_camp(None, 3_000);
        assert_eq!(w.state(), HomeState::Checking);
        assert_eq!(w.camp(), None);
    }

    #[test]
    fn plain_wifi_names_the_camp_nearest_the_last_fix() {
        let mut w = two_camps();
        w.on_fix(destination(camp_b(), 90.0, 5.0), 5.0, 1_000);
        w.on_wifi(Some(true), 2_000);
        assert_eq!((w.state(), w.camp()), (HomeState::Home, Some("b")));
        let mut none = two_camps();
        none.on_wifi(Some(true), 0);
        assert_eq!((none.state(), none.camp()), (HomeState::Home, None));
    }

    #[test]
    fn removing_the_camp_the_player_is_at_follows_at_once() {
        let mut w = two_camps();
        w.on_wifi_camp(Some("a".into()), 1_000);
        w.set_camps(vec![("b".into(), camp_b())]);
        assert_eq!(w.state(), HomeState::Home);
        assert_eq!(w.camp(), None, "no fix to say which camp, and b's Wi-Fi was not named");
        w.set_camps(vec![]);
        assert_eq!((w.state(), w.camp()), (HomeState::Home, None), "Wi-Fi alone: still home");
    }

    #[test]
    fn a_camp_is_named_only_within_its_circle() {
        let mut w = two_camps();
        w.on_fix(out(5.0), 5.0, 1_000);
        w.on_fix(out(5.0), 5.0, 2_000);
        assert_eq!(w.camp(), Some("a"));
        w.set_camps(vec![("far".into(), destination(home(), 0.0, 3_000.0))]);
        assert_eq!((w.state(), w.camp()), (HomeState::Home, None));
    }

    #[test]
    fn a_check_that_falls_back_home_names_the_wifi_camp() {
        let mut w = two_camps();
        w.on_wifi_camp(Some("a".into()), 0);
        walk(&mut w, 0, 1_000, 16);
        assert_eq!(w.state(), HomeState::Checking);
        w.on_wifi_camp(Some("b".into()), 20_000);
        w.tick(1_000 + CHECK_MS + 20_000);
        assert_eq!((w.state(), w.camp()), (HomeState::Home, Some("b")));
    }

    #[test]
    fn an_unknown_camp_wifi_is_plain_wifi() {
        let mut w = two_camps();
        w.on_wifi_camp(Some("gone".into()), 1_000);
        assert_eq!((w.state(), w.camp()), (HomeState::Home, None));
    }
}
