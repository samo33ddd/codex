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
}

impl OutputRateTracker {
    /// Reset the meter, e.g. at the start of a new turn.
    pub(crate) fn reset(&mut self) {
        self.buckets.clear();
    }

    /// Record `chars` output characters observed at `now`.
    pub(crate) fn record(&mut self, chars: usize, now: Instant) {
        if chars == 0 {
            return;
        }
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
}
