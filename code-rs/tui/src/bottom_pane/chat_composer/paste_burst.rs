use std::time::Duration;
use std::time::Instant;

// Heuristic thresholds for detecting paste-like input bursts.
// Detect quickly to avoid showing typed prefix before paste is recognized.
const PASTE_BURST_MIN_CHARS: u16 = 3;
const PASTE_BURST_CHAR_INTERVAL: Duration = Duration::from_millis(8);
const PASTE_ENTER_SUPPRESS_WINDOW: Duration = Duration::from_millis(120);
/// `PASTE_BURST_CHAR_INTERVAL` (8ms) plus a 1ms margin so the flush check
/// lands strictly after the burst interval.
const RECOMMENDED_FLUSH_DELAY: Duration = Duration::from_millis(9);

#[derive(Default)]
pub(crate) struct PasteBurst {
    last_plain_char_time: Option<Instant>,
    consecutive_plain_char_burst: u16,
    burst_window_until: Option<Instant>,
}

impl PasteBurst {
    /// Lightweight path: record an unmodified character keystroke to help
    /// detect paste-like bursts even when bracketed paste is unavailable.
    ///
    /// When several plain characters arrive within `PASTE_BURST_CHAR_INTERVAL`,
    /// we open a short window during which `Enter` will be treated as a newline
    /// insert instead of a submit. This prevents multi-line per-key pastes from
    /// firing off multiple submissions.
    pub fn record_plain_char_for_enter_window(&mut self, now: Instant) {
        let within_interval = self
            .last_plain_char_time
            .is_some_and(|prev| now.duration_since(prev) <= PASTE_BURST_CHAR_INTERVAL);

        match (self.last_plain_char_time, within_interval) {
            (_, true) => {
                // Saturates safely: a very large paste simply keeps the
                // suppression window open until `flush_if_due` retires it.
                self.consecutive_plain_char_burst =
                    self.consecutive_plain_char_burst.saturating_add(1);
            }
            _ => {
                self.consecutive_plain_char_burst = 1;
            }
        }
        self.last_plain_char_time = Some(now);

        if self.consecutive_plain_char_burst >= PASTE_BURST_MIN_CHARS {
            self.burst_window_until = Some(now + PASTE_ENTER_SUPPRESS_WINDOW);
        }
    }

    /// Return true when a recent burst suggests Enter should insert a newline
    /// instead of submitting the composer.
    #[must_use]
    pub fn enter_should_insert_newline(&self, now: Instant) -> bool {
        // Treat the suppression window as half-open [start, until). This makes
        // the expiry instant deterministic in tests and avoids "one extra
        // tick" behavior at the boundary.
        self.burst_window_until.is_some_and(|until| now < until)
    }

    /// True when the most recent plain char arrived within the burst interval.
    #[must_use]
    pub fn recent_plain_char(&self, now: Instant) -> bool {
        self.last_plain_char_time
            .is_some_and(|prev| now.duration_since(prev) <= PASTE_BURST_CHAR_INTERVAL)
    }

    /// Keep the Enter-suppression window alive for subsequent newlines in the
    /// same paste burst.
    pub fn extend_enter_window(&mut self, now: Instant) {
        self.burst_window_until = Some(now + PASTE_ENTER_SUPPRESS_WINDOW);
    }

    /// Clear the Enter guard when encountering non-character input paths.
    pub fn clear_enter_window(&mut self) {
        self.consecutive_plain_char_burst = 0;
        self.last_plain_char_time = None;
        self.burst_window_until = None;
    }

    /// Recommended delay before polling the burst detector again.
    #[must_use]
    pub fn recommended_flush_delay() -> Duration {
        RECOMMENDED_FLUSH_DELAY
    }

    /// Retire the timing window once it expires.
    ///
    /// Returns `true` when transient burst state was cleared so callers can
    /// request a final redraw if they care about that transition.
    #[must_use]
    pub fn flush_if_due(&mut self, now: Instant) -> bool {
        let burst_expired = self.burst_window_until.is_some_and(|until| now >= until);
        if burst_expired {
            self.clear_enter_window();
            return true;
        }

        // If we never opened the Enter-suppression window, stale timing state
        // can be dropped without asking callers to redraw.
        if self.burst_window_until.is_none()
            && self
                .last_plain_char_time
                .is_some_and(|prev| now.duration_since(prev) > PASTE_BURST_CHAR_INTERVAL)
        {
            self.last_plain_char_time = None;
            self.consecutive_plain_char_burst = 0;
        }

        false
    }

    /// Returns true while the Enter-suppression timing window is active.
    #[must_use]
    pub fn is_active(&self, now: Instant) -> bool {
        self.burst_window_until.is_some_and(|until| now < until)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flush_if_due_preserves_enter_window_until_suppress_timeout() {
        let start = Instant::now();
        let mut burst = PasteBurst::default();

        burst.record_plain_char_for_enter_window(start);
        burst.record_plain_char_for_enter_window(start + Duration::from_millis(1));
        burst.record_plain_char_for_enter_window(start + Duration::from_millis(2));

        let early = start + PASTE_BURST_CHAR_INTERVAL + Duration::from_millis(1);
        assert!(!burst.flush_if_due(early));
        assert!(burst.enter_should_insert_newline(early));

        let expired = start + Duration::from_millis(2) + PASTE_ENTER_SUPPRESS_WINDOW;
        assert!(burst.flush_if_due(expired));
        assert!(!burst.enter_should_insert_newline(expired));
    }

    #[test]
    fn slow_char_inside_suppress_window_does_not_clear_it() {
        let start = Instant::now();
        let mut burst = PasteBurst::default();

        burst.record_plain_char_for_enter_window(start);
        burst.record_plain_char_for_enter_window(start + Duration::from_millis(1));
        burst.record_plain_char_for_enter_window(start + Duration::from_millis(2));

        let still_within_window = start + Duration::from_millis(50);
        burst.record_plain_char_for_enter_window(still_within_window);

        assert!(burst.enter_should_insert_newline(still_within_window));
    }
}
