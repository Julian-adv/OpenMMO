use std::time::Instant;

/// Token bucket for client requests that cost the server real work.
pub(crate) struct RateLimiter {
    per_sec: f32,
    burst: f32,
    tokens: f32,
    last: Instant,
}

impl RateLimiter {
    pub(crate) fn new(per_sec: f32, burst: f32) -> Self {
        Self::starting_at(per_sec, burst, Instant::now())
    }

    pub(crate) fn starting_at(per_sec: f32, burst: f32, now: Instant) -> Self {
        Self {
            per_sec,
            burst,
            tokens: burst,
            last: now,
        }
    }

    pub(crate) fn allow(&mut self) -> bool {
        self.allow_at(Instant::now())
    }

    pub(crate) fn allow_at(&mut self, now: Instant) -> bool {
        let elapsed = now.saturating_duration_since(self.last).as_secs_f32();
        self.tokens = (self.tokens + elapsed * self.per_sec).min(self.burst);
        self.last = now;
        if self.tokens < 1.0 {
            return false;
        }
        self.tokens -= 1.0;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn a_burst_is_spent_then_refilled_at_the_rate() {
        let start = Instant::now();
        let mut limiter = RateLimiter::starting_at(1.0, 2.0, start);

        assert!(limiter.allow_at(start));
        assert!(limiter.allow_at(start));
        assert!(!limiter.allow_at(start + Duration::from_millis(999)));
        assert!(limiter.allow_at(start + Duration::from_millis(1000)));
        assert!(!limiter.allow_at(start + Duration::from_millis(1000)));
    }
}
