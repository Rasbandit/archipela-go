//! How often the host drains the Archipelago connection, and when the open game must sync with the server.
//!
//! The client library has no callbacks, so the host has to poll it; but only fast right after the server said something. Each quiet
//! poll waits longer, up to [`SLOW_MS`] (an item then shows up a few seconds late, which is nothing for a walking game). The game is
//! synced only when items or server data changed, or when the open game was never synced with this session.

/// The wait right after the server said something, in milliseconds.
pub const FAST_MS: u64 = 300;
/// The longest wait while the server is quiet, in milliseconds.
pub const SLOW_MS: u64 = 3_000;
const GROWTH_NUM: u64 = 3;
const GROWTH_DEN: u64 = 2;

/// The poll back-off: fast after activity, half as fast again on each quiet poll, never slower than [`SLOW_MS`].
#[derive(Debug, Clone)]
pub struct PollBackoff {
    delay_ms: u64,
}

impl Default for PollBackoff {
    fn default() -> Self {
        Self { delay_ms: FAST_MS }
    }
}

impl PollBackoff {
    /// The wait before the next poll, given whether this one brought any events.
    pub fn next(&mut self, active: bool) -> u64 {
        self.delay_ms = if active { FAST_MS } else { (self.delay_ms * GROWTH_NUM / GROWTH_DEN).min(SLOW_MS) };
        self.delay_ms
    }
}

/// Whether the open game must sync with the server: no game is open (`None`) means nothing to sync; otherwise items or server data
/// changed, or this game is not the one last synced with the session (newly opened, or reopened after a pause).
#[must_use]
pub fn needs_sync(server_changed: bool, synced_game: Option<&str>, open_game: Option<&str>) -> bool {
    open_game.is_some_and(|g| server_changed || synced_game != Some(g))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn polls_fast_right_after_activity_and_slows_down_while_quiet() {
        let mut p = PollBackoff::default();
        assert_eq!(p.next(true), FAST_MS);
        let quiet: Vec<u64> = (0..20).map(|_| p.next(false)).collect();
        assert!(quiet.windows(2).all(|w| w[1] >= w[0]), "never faster while quiet: {quiet:?}");
        assert_eq!(*quiet.last().unwrap(), SLOW_MS, "settles at the slow rate");
        assert_eq!(p.next(true), FAST_MS, "activity again: fast at once");
    }

    #[test]
    fn the_open_game_syncs_on_server_changes_or_when_it_is_not_the_one_last_synced() {
        assert!(needs_sync(false, None, Some("g")), "never synced with this session");
        assert!(needs_sync(true, Some("g"), Some("g")), "items or data arrived");
        assert!(!needs_sync(false, Some("g"), Some("g")), "nothing new");
        assert!(needs_sync(false, Some("a"), Some("b")), "another game was opened");
        assert!(!needs_sync(true, Some("g"), None), "no game open: nothing to sync");
    }
}
