//! When the open game is written to disk: at once on events, otherwise on a slow clock, so counters survive a kill.

/// Decides when the open game should be written to disk: at once when something happened, otherwise at most every `interval_ms`.
pub struct SavePolicy {
    interval_ms: i64,
    last_ms: Option<i64>,
}

impl SavePolicy {
    /// A policy that saves at least every `interval_ms` milliseconds.
    #[must_use]
    pub fn new(interval_ms: i64) -> Self {
        Self { interval_ms, last_ms: None }
    }

    /// `eventful`: this call produced game events (always save). Returns true when the caller should save now and records it.
    pub fn due(&mut self, now_ms: i64, eventful: bool) -> bool {
        let due = eventful || self.last_ms.is_none_or(|last| now_ms < last || now_ms - last >= self.interval_ms);
        if due {
            self.last_ms = Some(now_ms);
        }
        due
    }

    /// Forget the last save (a different game was opened).
    pub fn reset(&mut self) {
        self.last_ms = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_call_is_due() {
        assert!(SavePolicy::new(30_000).due(1_000, false));
    }

    #[test]
    fn calls_inside_the_interval_are_not_due() {
        let mut p = SavePolicy::new(30_000);
        assert!(p.due(0, false));
        assert!(!p.due(10_000, false));
        assert!(!p.due(29_999, false));
    }

    #[test]
    fn eventful_call_is_due_and_restarts_the_interval() {
        let mut p = SavePolicy::new(30_000);
        p.due(0, false);
        assert!(p.due(10_000, true));
        assert!(!p.due(39_999, false));
        assert!(p.due(40_000, false));
    }

    #[test]
    fn exactly_the_interval_later_is_due() {
        let mut p = SavePolicy::new(30_000);
        p.due(5_000, false);
        assert!(p.due(35_000, false));
    }

    #[test]
    fn clock_going_backwards_is_due() {
        let mut p = SavePolicy::new(30_000);
        p.due(100_000, false);
        assert!(p.due(50_000, false));
        assert!(!p.due(60_000, false));
    }

    #[test]
    fn reset_makes_the_next_call_due() {
        let mut p = SavePolicy::new(30_000);
        p.due(0, false);
        p.reset();
        assert!(p.due(1_000, false));
    }
}
