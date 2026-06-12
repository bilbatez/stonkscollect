//! Network resilience primitives: retry/backoff policy + a rate limiter.
//! Pure and deterministic (time injected) so they're fully testable; the real
//! HTTP loop that uses them lives in `http.rs` (excluded glue).

use std::collections::HashMap;
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
        let mut next = self.next_slot.lock().expect("rate limiter mutex poisoned");
        let slot = match *next {
            Some(t) if t > now => t,
            _ => now,
        };
        *next = Some(slot + interval);
        (slot - now).to_std().unwrap_or(Duration::ZERO)
    }
}

/// HTTP cache validators (RFC 9110): an entity tag and/or `Last-Modified`
/// stamp, replayed as conditional-request headers so an unchanged upstream
/// document answers 304 instead of a full body.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Validators {
    pub etag: Option<String>,
    pub last_modified: Option<String>,
}

impl Validators {
    /// The conditional-request headers for these validators (only the present
    /// fields; empty validators mean an unconditional request).
    pub fn request_headers(&self) -> Vec<(&'static str, String)> {
        let mut headers = Vec::new();
        if let Some(etag) = &self.etag {
            headers.push(("If-None-Match", etag.clone()));
        }
        if let Some(last_modified) = &self.last_modified {
            headers.push(("If-Modified-Since", last_modified.clone()));
        }
        headers
    }
}

/// Persistence for per-URL cache validators. Best-effort by contract: a failing
/// cache must never fail a collection, so reads degrade to "no validators" and
/// writes are fire-and-forget.
#[async_trait::async_trait(?Send)]
pub trait HttpValidatorRepo {
    /// Stored validators for `url`, if any.
    async fn validators(&self, url: &str) -> Option<Validators>;
    /// Persist the validators a fresh response arrived with.
    async fn store_validators(&self, url: &str, validators: &Validators);
}

/// Default brute-force policy for interactive login: this many failures per key
/// within the window trips the block.
pub const LOGIN_MAX_ATTEMPTS: u32 = 5;
pub const LOGIN_WINDOW: Duration = Duration::from_secs(15 * 60);

/// One key's recent failed-login tally and the start of its current window.
struct Attempt {
    count: u32,
    first: DateTime<Utc>,
}

/// In-memory failed-login throttle keyed by identity (e.g. email). After
/// `max` failures inside `window` a key is blocked until the window elapses;
/// a successful login clears it. Transient and per-process (not persisted) —
/// brute-force mitigation, not durable account lockout. Keyed by email, so a
/// flood against one account only locks that account, never the whole service.
pub struct LoginThrottle {
    max: u32,
    window: Duration,
    inner: Mutex<HashMap<String, Attempt>>,
}

impl LoginThrottle {
    pub fn new(max: u32, window: Duration) -> Self {
        Self { max, window, inner: Mutex::new(HashMap::new()) }
    }

    /// Whether `first` is recent enough that its window still covers `now`.
    fn within_window(&self, first: DateTime<Utc>, now: DateTime<Utc>) -> bool {
        now.signed_duration_since(first).to_std().is_ok_and(|e| e < self.window)
    }

    /// Whether a login attempt for `key` is currently permitted.
    pub fn allowed(&self, key: &str, now: DateTime<Utc>) -> bool {
        let map = self.inner.lock().expect("login throttle mutex poisoned");
        !matches!(map.get(key), Some(a) if a.count >= self.max && self.within_window(a.first, now))
    }

    /// Record a failed attempt for `key`, starting a fresh window if the prior
    /// one has elapsed.
    pub fn record_failure(&self, key: &str, now: DateTime<Utc>) {
        let mut map = self.inner.lock().expect("login throttle mutex poisoned");
        let a = map.entry(key.to_string()).or_insert(Attempt { count: 0, first: now });
        if !self.within_window(a.first, now) {
            a.count = 0;
            a.first = now;
        }
        a.count += 1;
    }

    /// Clear a key's failure history (call on a successful login).
    pub fn clear(&self, key: &str) {
        self.inner.lock().expect("login throttle mutex poisoned").remove(key);
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
    fn login_throttle_blocks_after_max_failures_within_window() {
        let th = LoginThrottle::new(3, Duration::from_secs(900));
        let now = t(0);
        assert!(th.allowed("a@e.com", now)); // no history
        th.record_failure("a@e.com", now);
        th.record_failure("a@e.com", now);
        assert!(th.allowed("a@e.com", now)); // 2 < 3, still allowed
        th.record_failure("a@e.com", now);
        assert!(!th.allowed("a@e.com", now)); // 3 >= 3, blocked
        // a different key is unaffected
        assert!(th.allowed("b@e.com", now));
    }

    #[test]
    fn login_throttle_resets_after_window_and_on_success() {
        let th = LoginThrottle::new(2, Duration::from_secs(900));
        let now = t(0);
        th.record_failure("a@e.com", now);
        th.record_failure("a@e.com", now);
        assert!(!th.allowed("a@e.com", now)); // blocked
        assert!(th.allowed("a@e.com", t(901))); // window elapsed -> allowed
        // a failure after the window resets the counter
        th.record_failure("a@e.com", t(901));
        assert!(th.allowed("a@e.com", t(901))); // count back to 1 < 2
        th.record_failure("a@e.com", t(901));
        assert!(!th.allowed("a@e.com", t(901))); // blocked again

        // success clears the key
        th.record_failure("b@e.com", now);
        th.record_failure("b@e.com", now);
        assert!(!th.allowed("b@e.com", now));
        th.clear("b@e.com");
        assert!(th.allowed("b@e.com", now));
    }

    #[test]
    fn validators_request_headers_cover_only_present_fields() {
        assert!(Validators::default().request_headers().is_empty());
        let etag_only = Validators { etag: Some("\"abc\"".into()), last_modified: None };
        assert_eq!(
            etag_only.request_headers(),
            vec![("If-None-Match", "\"abc\"".to_string())]
        );
        let both = Validators {
            etag: Some("\"abc\"".into()),
            last_modified: Some("Tue, 01 Jan 2024 00:00:00 GMT".into()),
        };
        assert_eq!(
            both.request_headers(),
            vec![
                ("If-None-Match", "\"abc\"".to_string()),
                ("If-Modified-Since", "Tue, 01 Jan 2024 00:00:00 GMT".to_string()),
            ]
        );
        assert_eq!(both.clone(), both);
        assert!(format!("{both:?}").contains("abc"));
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
