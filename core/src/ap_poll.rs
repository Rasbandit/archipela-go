//! How often the host drains the Archipelago connection, and when the open game must sync with the server.
//!
//! While connected the host sleeps until the server sends something (`ApSession::wait_for_data`); this back-off times the drains
//! right after activity and while connecting: fast right after the server said something, slower on each quiet poll, up to
//! [`SLOW_MS`]. [`ReconnectBackoff`] times reconnects of a dropped session. The game is synced only when items or server data
//! changed, or when the open game was never synced with this session.

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

/// The first wait before reconnecting a lost connection, ms (Archipelago's `CommonClient`: 5 s, doubling).
pub const RECONNECT_FIRST_MS: u64 = 5_000;
/// The longest wait between reconnect tries, ms: the reference client has no cap; a phone saves its battery while a server is down.
pub const RECONNECT_MAX_MS: u64 = 300_000;

/// When to try reconnecting a session that was connected and dropped ("reconnect if the connection is lost while playing", a hard
/// client requirement): 5 s, doubling, never slower than [`RECONNECT_MAX_MS`]; reset once connected again.
#[derive(Debug, Clone, Default)]
pub struct ReconnectBackoff {
    next_ms: Option<u64>,
}

impl ReconnectBackoff {
    /// The wait before the next try.
    pub fn next_wait_ms(&mut self) -> u64 {
        let wait = self.next_ms.unwrap_or(RECONNECT_FIRST_MS);
        self.next_ms = Some((wait * 2).min(RECONNECT_MAX_MS));
        wait
    }

    /// Connected again: the next drop starts from the first wait.
    pub fn reset(&mut self) {
        self.next_ms = None;
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
    fn reconnects_like_the_reference_client_but_never_slower_than_five_minutes() {
        let mut r = ReconnectBackoff::default();
        let waits: Vec<u64> = (0..10).map(|_| r.next_wait_ms()).collect();
        assert_eq!(&waits[..4], &[5_000, 10_000, 20_000, 40_000], "CommonClient: 5 s, doubling");
        assert_eq!(*waits.last().unwrap(), RECONNECT_MAX_MS, "capped: a server that is down for hours costs one try per 5 min");
        r.reset();
        assert_eq!(r.next_wait_ms(), 5_000, "connected again: start over");
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
