//! Rolling estimate of model output speed shown in the TUI status row.
//!
//! Streaming deltas arrive as text fragments without per-delta token counts, so
//! the rate is approximated from received characters (`~4 chars per token`, the
//! common English-heavy heuristic). A sliding window keeps the displayed value
//! responsive to speed changes instead of averaging over the whole turn.

use std::collections::VecDeque;
use std::time::Duration;
use std::time::Instant;

/// Approximate number of UTF-8 characters per model token.
const CHARS_PER_TOKEN: f64 = 4.0;
/// Width of the sliding measurement window.
const WINDOW: Duration = Duration::from_secs(5);
/// Buckets used inside the window; each bucket covers 500 ms.
const BUCKET_DURATION: Duration = Duration::from_millis(500);
/// Show "~N tok/s" once at least this much output has been observed.
const MIN_OBSERVED_CHARS: usize = 8;

/// Tracks output characters received during the current turn.
///
/// `record` is called from every text-delta path (assistant message, plan and
/// reasoning deltas). `tokens_per_second` collapses the per-bucket totals into
/// a single rate over the active part of the window.
#[derive(Debug, Default)]
pub(crate) struct OutputRateTracker {
    buckets: VecDeque<(Instant, usize)>,
    /// First recorded output time this turn, if any. Used for the whole-turn
    /// average rate in the completion separator.
    started_at: Option<Instant>,
    /// Total characters recorded this turn (not just the sliding window).
    total_chars: usize,
}

impl OutputRateTracker {
    /// Reset the meter, e.g. at the start of a new turn.
    pub(crate) fn reset(&mut self) {
        self.buckets.clear();
        self.started_at = None;
        self.total_chars = 0;
    }

    /// Record `chars` output characters observed at `now`.
    pub(crate) fn record(&mut self, chars: usize, now: Instant) {
        if chars == 0 {
            return;
        }
        self.started_at.get_or_insert(now);
        self.total_chars += chars;
        self.evict(now);
        if let Some((bucket_start, count)) = self.buckets.back_mut()
            && now.duration_since(*bucket_start) < BUCKET_DURATION
        {
            *count += chars;
            return;
        }
        let bucket_start = self
            .buckets
            .back()
            .map(|(start, _)| *start + BUCKET_DURATION)
            .unwrap_or(now);
        self.buckets.push_back((bucket_start, chars));
    }

    /// Estimated output tokens per second over the active window.
    ///
    /// Returns `None` until at least [`MIN_OBSERVED_CHARS`] characters have
    /// been recorded, so the indicator does not flash noisy tiny values.
    pub(crate) fn tokens_per_second(&self, now: Instant) -> Option<f64> {
        let first_bucket = self.buckets.front()?;
        let total_chars: usize = self.buckets.iter().map(|(_, count)| count).sum();
        if total_chars < MIN_OBSERVED_CHARS {
            return None;
        }
        // Measure over the time actually covered by recorded buckets, from the
        // first surviving bucket to `now`. At least one bucket duration so a
        // single large burst does not spike, and never more than the window.
        let elapsed = now.saturating_duration_since(first_bucket.0);
        let measured = elapsed.max(BUCKET_DURATION).min(WINDOW);
        Some(total_chars as f64 / CHARS_PER_TOKEN / measured.as_secs_f64())
    }

    /// Whole-turn average tokens/sec, for the completion separator.
    ///
    /// `turn_started_at` is when the turn actually began — for providers that
    /// only send a final item the tracker may see all output in a single
    /// instant, so its own `started_at` cannot measure a meaningful span.
    /// Returns `None` when nothing was recorded this turn or the elapsed time is
    /// too short to be meaningful. This is an average over the whole turn, not
    /// a live rate, and shows as such in the UI.
    pub(crate) fn turn_average_tokens_per_second(
        &self,
        turn_started_at: Instant,
        now: Instant,
    ) -> Option<f64> {
        if self.total_chars < MIN_OBSERVED_CHARS {
            return None;
        }
        let secs = now
            .saturating_duration_since(turn_started_at)
            .as_secs_f64();
        if secs <= 0.0 {
            return None;
        }
        Some(self.total_chars as f64 / CHARS_PER_TOKEN / secs)
    }

    /// Drop buckets older than the measurement window.
    fn evict(&mut self, now: Instant) {
        while let Some((bucket_start, _)) = self.buckets.front() {
            if now.duration_since(*bucket_start) >= WINDOW {
                self.buckets.pop_front();
            } else {
                break;
            }
        }
    }

    /// Whether any output has been recorded this turn.
    ///
    /// Used by completion-time fallbacks: when the provider only sends a final
    /// item (no streamed deltas), we still want the whole-turn average.
    pub(crate) fn has_output(&self) -> bool {
        self.total_chars > 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_rate_after_enough_output() {
        let mut tracker = OutputRateTracker::default();
        let now = Instant::now();
        // Elapsed from the first bucket to `now` is 2s: 400 / 4 / 2 = 50 tok/s.
        tracker.record(200, now);
        tracker.record(200, now + Duration::from_secs(2));
        let rate = tracker
            .tokens_per_second(now + Duration::from_secs(2))
            .expect("rate");
        assert!((rate - 50.0).abs() < 0.5, "unexpected rate {rate}");
    }

    #[test]
    fn hidden_until_min_output() {
        let mut tracker = OutputRateTracker::default();
        let now = Instant::now();
        tracker.record(4, now);
        assert!(
            tracker
                .tokens_per_second(now + Duration::from_secs(1))
                .is_none()
        );
    }

    #[test]
    fn window_caps_at_five_seconds() {
        let mut tracker = OutputRateTracker::default();
        let now = Instant::now();
        // Old output outside the window is evicted.
        tracker.record(400, now);
        tracker.record(200, now + Duration::from_secs(6));
        let rate = tracker
            .tokens_per_second(now + Duration::from_secs(6))
            .expect("rate");
        // Only the second burst survives in the window; it is measured over
        // the 500ms bucket floor: 200 / 4 / 0.5 = 100 tok/s.
        assert!((rate - 100.0).abs() < 0.5, "unexpected rate {rate}");
    }

    #[test]
    fn reset_clears_state() {
        let mut tracker = OutputRateTracker::default();
        tracker.record(400, Instant::now());
        tracker.reset();
        assert!(tracker.tokens_per_second(Instant::now()).is_none());
    }

    #[test]
    fn turn_average_uses_whole_turn() {
        let mut tracker = OutputRateTracker::default();
        let turn_started_at = Instant::now();
        // Streamed output arrives throughout the turn; completion lands 8s
        // after it started -> 400/4/8 = 12.5 tok/s regardless of the window.
        tracker.record(100, turn_started_at + Duration::from_secs(1));
        tracker.record(300, turn_started_at + Duration::from_secs(6));
        let completion_at = turn_started_at + Duration::from_secs(8);
        let rate = tracker
            .turn_average_tokens_per_second(turn_started_at, completion_at)
            .expect("avg rate");
        assert!((rate - 12.5).abs() < 0.1, "unexpected avg rate {rate}");
    }

    #[test]
    fn turn_average_none_without_output() {
        let tracker = OutputRateTracker::default();
        assert!(
            tracker
                .turn_average_tokens_per_second(Instant::now(), Instant::now())
                .is_none()
        );
    }

    #[test]
    fn turn_average_uses_turn_start_when_output_is_instant() {
        // Provider sent only a final ItemCompleted: all text arrives at once,
        // but the turn itself ran for 10s — the rate should reflect that.
        let mut tracker = OutputRateTracker::default();
        let turn_started_at = Instant::now();
        let completion_at = turn_started_at + Duration::from_secs(10);
        tracker.record(400, completion_at);
        let rate = tracker
            .turn_average_tokens_per_second(turn_started_at, completion_at)
            .expect("avg rate");
        assert!((rate - 10.0).abs() < 0.1, "unexpected avg rate {rate}");
    }
}
