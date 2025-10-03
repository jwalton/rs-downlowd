pub struct LeakyBucket {
    /// The target maximum bytes per second.
    pub max_bytes_per_second: u64,

    /// The last time we tried to take some tokens from the bucket.
    last: std::time::Instant,
    /// The number of tokens available in the bucket.
    bytes_available: u64,
}

impl LeakyBucket {
    /// Create a new limiter that allows approximately `max_bytes_per_second`
    /// kilobytes per second to be downloaded.
    pub fn new(max_bytes_per_second: u64) -> Self {
        Self {
            max_bytes_per_second,
            last: std::time::Instant::now(),
            bytes_available: 0,
        }
    }

    /// Returns the duration of time to wait before `bytes` can be downloaded.
    pub fn bytes_consumed(&mut self, bytes: u64) -> Option<std::time::Duration> {
        let now = std::time::Instant::now();
        let (elapsed, extra_delay) = if self.last > now {
            // We're still supposed to be waiting from last time.
            (
                std::time::Duration::ZERO,
                Some(self.last.duration_since(now)),
            )
        } else {
            (now.duration_since(self.last), None)
        };

        let elapsed = elapsed.as_millis();
        let delay = self.bytes_consumed_inner(bytes, elapsed);

        // Update the last time we consumed bytes. If we are delaying, add the delay.
        self.last = now + delay.unwrap_or_default();

        match (delay, extra_delay) {
            (Some(d), Some(ed)) => Some(d + ed),
            (Some(d), None) => Some(d),
            (None, Some(ed)) => Some(ed),
            (None, None) => None,
        }
    }

    fn bytes_consumed_inner(
        &mut self,
        bytes: u64,
        elapsed_ms: u128,
    ) -> Option<std::time::Duration> {
        // If we have no limit, then we'd end up waiting forever, so always allow
        // a tiny trickle.
        let max_bps = self.max_bytes_per_second.max(1);

        if elapsed_ms > 1000 {
            // If the elapsed time is too large, just reset the bucket.
            self.bytes_available = max_bps;
        } else {
            // Add tokens to the bucket based on the elapsed time.
            let bytes_elapsed = (elapsed_ms as u64).saturating_mul(max_bps) / 1000;
            self.bytes_available = self
                .bytes_available
                .saturating_add(bytes_elapsed)
                .min(max_bps);
        }

        if self.bytes_available >= bytes {
            self.bytes_available -= bytes;
            None
        } else {
            let bytes_needed = bytes - self.bytes_available;
            self.bytes_available = 0;
            let wait_millis = bytes_needed.saturating_mul(1000) / max_bps;

            let wait_duration = std::time::Duration::from_millis(wait_millis);
            Some(wait_duration)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_not_wait_if_consuming_slow_enough() {
        let mut limiter = LeakyBucket::new(100);
        assert_eq!(limiter.bytes_consumed_inner(100, 1000), None);
        assert_eq!(limiter.bytes_consumed_inner(100, 1000), None);
        assert_eq!(limiter.bytes_consumed_inner(100, 1000), None);
        assert_eq!(limiter.bytes_consumed_inner(50, 1000), None);
        assert_eq!(limiter.bytes_consumed_inner(10, 100), None);
        assert_eq!(limiter.bytes_consumed_inner(10, 100), None);
        assert_eq!(limiter.bytes_consumed_inner(10, 100), None);
        assert_eq!(limiter.bytes_consumed_inner(0, 1000), None);
        assert_eq!(limiter.bytes_consumed_inner(0, 0), None);
    }

    #[test]
    fn should_wait_if_we_start_consuming_too_fast() {
        let mut limiter = LeakyBucket::new(100);
        assert_eq!(
            limiter.bytes_consumed_inner(110, 1000),
            Some(std::time::Duration::from_millis(100))
        );
        assert_eq!(
            limiter.bytes_consumed_inner(10, 0),
            Some(std::time::Duration::from_millis(100))
        );
        assert_eq!(limiter.bytes_consumed_inner(100, 1000), None);

        // This should still wait, even though a long time has passed, because
        // we don't burst above the max.
        assert_eq!(
            limiter.bytes_consumed_inner(110, 100000),
            Some(std::time::Duration::from_millis(100))
        );
    }

    #[test]
    fn should_work_for_big_numbers() {
        let mut limiter = LeakyBucket::new(100);

        assert_eq!(
            limiter.bytes_consumed_inner(100_000_000, 0),
            Some(std::time::Duration::from_secs(1_000_000))
        );

        assert_eq!(limiter.bytes_consumed_inner(100, 1000), None);

        assert_eq!(limiter.bytes_consumed_inner(100, u128::MAX), None);

        // This is going to return something very large and nonsensical, but
        // it shouldn't overflow.
        assert!(
            limiter
                .bytes_consumed_inner(u64::MAX, u128::MAX)
                .unwrap()
                .as_secs()
                > 1_000_000
        );
    }
}
