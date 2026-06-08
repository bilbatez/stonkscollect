//! Network resilience primitives: retry/backoff policy + a rate limiter.
//! Pure and deterministic (time injected) so they're fully testable; the real
//! HTTP loop that uses them lives in `http.rs` (excluded glue).

use std::sync::Mutex;
use std::time::Duration;

use chrono::{DateTime, Utc};

/// Exponential backoff with a cap, honoring `Retry-After` when present.
#[derive(Debug, Clone)]
pub struct RetryPolicy {
    pub max_retries: u32,
    pub base: Duration,
    pub max: Duration,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_retries: 4,
            base: Duration::from_millis(250),
            max: Duration::from_secs(10),
        }
    }
}

impl RetryPolicy {
    /// How long to wait before the given retry `attempt` (0-based).
    pub fn delay_for(&self, attempt: u32, retry_after: Option<Duration>) -> Duration {
        if let Some(ra) = retry_after {
            return ra.min(self.max);
        }
        let factor = 2u32.saturating_pow(attempt);
        self.base.saturating_mul(factor).min(self.max)
    }

    /// Whether to retry: only while attempts remain, and only for transient
    /// failures (transport error = no status, 429, or any 5xx).
    pub fn should_retry(&self, attempt: u32, status: Option<u16>) -> bool {
        if attempt >= self.max_retries {
            return false;
        }
        match status {
            None => true,
            Some(429) => true,
            Some(s) => s >= 500,
        }
    }
}

/// Spaces requests at least `min_interval` apart across all callers.
pub struct RateLimiter {
    min_interval: Duration,
    next_slot: Mutex<Option<DateTime<Utc>>>,
}

impl RateLimiter {
    pub fn new(min_interval: Duration) -> Self {
        Self {
            min_interval,
            next_slot: Mutex::new(None),
        }
    }

    /// Reserve the next slot relative to `now`; returns how long to sleep first.
    pub fn reserve(&self, now: DateTime<Utc>) -> Duration {
        let interval = chrono::Duration::from_std(self.min_interval).unwrap_or(chrono::Duration::zero());
        let mut next = self.next_slot.lock().unwrap();
        let slot = match *next {
            Some(t) if t > now => t,
            _ => now,
        };
        *next = Some(slot + interval);
        (slot - now).to_std().unwrap_or(Duration::ZERO)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn t(secs: i64) -> DateTime<Utc> {
        Utc.timestamp_opt(secs, 0).single().unwrap()
    }

    #[test]
    fn delay_for_is_exponential_and_capped() {
        let p = RetryPolicy {
            max_retries: 6,
            base: Duration::from_millis(100),
            max: Duration::from_secs(2),
        };
        assert_eq!(p.delay_for(0, None), Duration::from_millis(100));
        assert_eq!(p.delay_for(1, None), Duration::from_millis(200));
        assert_eq!(p.delay_for(3, None), Duration::from_millis(800));
        assert_eq!(p.delay_for(10, None), Duration::from_secs(2)); // capped
    }

    #[test]
    fn delay_for_honors_retry_after_capped() {
        let p = RetryPolicy::default();
        assert_eq!(p.delay_for(0, Some(Duration::from_secs(1))), Duration::from_secs(1));
        assert_eq!(p.delay_for(0, Some(Duration::from_secs(60))), p.max); // capped
    }

    #[test]
    fn should_retry_only_transient_and_within_budget() {
        let p = RetryPolicy { max_retries: 3, ..RetryPolicy::default() };
        assert!(p.should_retry(0, None)); // transport error
        assert!(p.should_retry(1, Some(429)));
        assert!(p.should_retry(2, Some(503)));
        assert!(!p.should_retry(0, Some(404))); // client error, no retry
        assert!(!p.should_retry(3, None)); // budget exhausted
    }

    #[test]
    fn rate_limiter_spaces_requests() {
        let rl = RateLimiter::new(Duration::from_secs(10));
        assert_eq!(rl.reserve(t(0)), Duration::ZERO); // first is immediate
        assert_eq!(rl.reserve(t(0)), Duration::from_secs(10)); // must wait a slot
        assert_eq!(rl.reserve(t(0)), Duration::from_secs(20)); // and another
        // a caller arriving after the slot has passed waits nothing
        assert_eq!(rl.reserve(t(100)), Duration::ZERO);
    }
}
