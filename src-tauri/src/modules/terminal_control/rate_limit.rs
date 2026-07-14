use std::time::Instant;

pub const TOKENS_PER_SECOND: f64 = 20.0;
pub const BURST_TOKENS: f64 = 40.0;

#[derive(Debug, Clone)]
pub struct TokenBucket {
    refill_per_second: f64,
    capacity: f64,
    tokens: f64,
    last_refill: Instant,
}

impl TokenBucket {
    pub fn new(refill_per_second: f64, capacity: f64, now: Instant) -> Self {
        let refill_per_second = finite_nonnegative(refill_per_second);
        let capacity = finite_nonnegative(capacity);
        Self {
            refill_per_second,
            capacity,
            tokens: capacity,
            last_refill: now,
        }
    }

    pub fn messaging(now: Instant) -> Self {
        Self::new(TOKENS_PER_SECOND, BURST_TOKENS, now)
    }

    pub fn take(&mut self, now: Instant) -> bool {
        if let Some(elapsed) = now.checked_duration_since(self.last_refill) {
            self.tokens =
                (self.tokens + elapsed.as_secs_f64() * self.refill_per_second).min(self.capacity);
            self.last_refill = now;
        }

        if self.tokens < 1.0 {
            return false;
        }

        self.tokens -= 1.0;
        true
    }
}

fn finite_nonnegative(value: f64) -> f64 {
    if value.is_finite() {
        value.max(0.0)
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    fn token_bucket_allows_burst_then_rejects() {
        let now = Instant::now();
        let mut bucket = TokenBucket::new(20.0, 40.0, now);
        for _ in 0..40 {
            assert!(bucket.take(now));
        }
        assert!(!bucket.take(now));
    }

    #[test]
    fn token_bucket_refills_at_the_configured_rate_up_to_burst() {
        let start = Instant::now();
        let mut bucket = TokenBucket::new(20.0, 40.0, start);
        for _ in 0..40 {
            assert!(bucket.take(start));
        }

        assert!(bucket.take(start + Duration::from_millis(50)));
        assert!(!bucket.take(start + Duration::from_millis(50)));

        let later = start + Duration::from_secs(10);
        for _ in 0..40 {
            assert!(bucket.take(later));
        }
        assert!(!bucket.take(later));
    }

    #[test]
    fn token_bucket_does_not_refill_when_clock_moves_backwards() {
        let start = Instant::now();
        let mut bucket = TokenBucket::new(1.0, 1.0, start);
        assert!(bucket.take(start));
        assert!(!bucket.take(start - Duration::from_millis(1)));
    }
}
