//! Local request budget, shared by every Binance client in the app.
//!
//! Binance answers a client that exceeds its weight allowance with an IP ban, not an error
//! code, so the budget is enforced here before a request is made rather than reacted to
//! afterwards. Lifted out of `binance_rest.rs` unchanged when the futures client needed the
//! same guard against its own, separate allowance.

use std::sync::Mutex;
use std::time::Instant;

/// Simple token bucket: refills `rate` weight/sec, caps at `capacity`.
pub struct TokenBucket {
    tokens: f64,
    capacity: f64,
    rate_per_sec: f64,
    last_refill: Instant,
}

impl TokenBucket {
    pub fn new(capacity: f64, rate_per_sec: f64) -> Self {
        Self {
            tokens: capacity,
            capacity,
            rate_per_sec,
            last_refill: Instant::now(),
        }
    }

    pub fn try_take(&mut self, cost: f64) -> bool {
        self.try_take_at(cost, Instant::now())
    }

    /// Same as [`TokenBucket::try_take`] with the clock injected, so refill behaviour can be
    /// tested without sleeping.
    fn try_take_at(&mut self, cost: f64, now: Instant) -> bool {
        let elapsed = now.saturating_duration_since(self.last_refill).as_secs_f64();
        self.tokens = (self.tokens + elapsed * self.rate_per_sec).min(self.capacity);
        self.last_refill = now;
        if self.tokens >= cost {
            self.tokens -= cost;
            true
        } else {
            false
        }
    }
}

/// A [`TokenBucket`] behind a lock, which is how every caller actually holds one.
pub struct WeightLimiter {
    bucket: Mutex<TokenBucket>,
    label: &'static str,
}

impl WeightLimiter {
    pub fn new(label: &'static str, capacity: f64, rate_per_sec: f64) -> Self {
        Self {
            bucket: Mutex::new(TokenBucket::new(capacity, rate_per_sec)),
            label,
        }
    }

    pub fn take(&self, weight: f64) -> Result<(), String> {
        if self.bucket.lock().unwrap().try_take(weight) {
            Ok(())
        } else {
            Err(format!("{} rate limit reached, backing off", self.label))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn spends_down_to_empty_then_refuses() {
        let mut bucket = TokenBucket::new(10.0, 1.0);
        assert!(bucket.try_take(6.0));
        assert!(bucket.try_take(4.0));
        assert!(!bucket.try_take(1.0), "the bucket is empty");
    }

    #[test]
    fn refills_at_the_configured_rate() {
        let start = Instant::now();
        let mut bucket = TokenBucket::new(10.0, 2.0);
        assert!(bucket.try_take_at(10.0, start));
        assert!(!bucket.try_take_at(1.0, start));

        // Two seconds later, 4 weight has accrued.
        let later = start + Duration::from_secs(2);
        assert!(bucket.try_take_at(4.0, later));
        assert!(!bucket.try_take_at(0.5, later));
    }

    #[test]
    fn refill_never_exceeds_capacity() {
        let start = Instant::now();
        let mut bucket = TokenBucket::new(10.0, 5.0);
        // An hour of idling must not bank an hour's worth of requests.
        let much_later = start + Duration::from_secs(3600);
        assert!(bucket.try_take_at(10.0, much_later));
        assert!(!bucket.try_take_at(0.1, much_later));
    }

    #[test]
    fn the_limiter_reports_which_budget_ran_out() {
        let limiter = WeightLimiter::new("futures", 2.0, 0.0);
        assert!(limiter.take(2.0).is_ok());
        let err = limiter.take(1.0).unwrap_err();
        assert!(err.contains("futures"), "{err}");
    }
}
