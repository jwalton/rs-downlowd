pub struct LeakyBucket {
    /// The target maximum bytes per second.
    pub max_bytes_per_second: u64,

    /// The last time we tried to take some tokens from the bucket.
    last: std::time::Instant,
    /// The number of tokens available in the bucket.  We slowly fill the bucket
    /// over time, up to a maximum of `max_bytes_per_second`.  Because we only
    /// find out how many bytes were downloaded after the fact, this can go
    /// negative to show we're "in debt".
    bytes_available: i128,
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

    /// Let the LeakyBucket know we downloaded some bytes. Returns the duration
    /// of time to wait before `bytes` can be downloaded.
    pub fn bytes_consumed(&mut self, bytes: u64) -> Option<std::time::Duration> {
        // Work out how long it's been since someone called this.
        let now = std::time::Instant::now();
        let elapsed = now.duration_since(self.last);
        let elapsed = elapsed.as_millis();
        self.last = now;

        self.bytes_consumed_inner(bytes, elapsed)
    }

    fn bytes_consumed_inner(&mut self, bytes: u64, elapsed: u128) -> Option<std::time::Duration> {
        // If we have no limit, then we'd end up waiting forever, so always allow
        // a tiny trickle.
        let max_bps = (self.max_bytes_per_second as i128).max(1);

        if elapsed > i128::MAX as u128 {
            // If the elapsed time is too large, just reset the bucket.
            self.bytes_available = max_bps;
        } else {
            // Add tokens to the bucket based on the elapsed time.
            let bytes_elapsed = (elapsed as i128).saturating_mul(max_bps) / 1000;
            self.bytes_available = self
                .bytes_available
                .saturating_add(bytes_elapsed)
                .min(max_bps);
        }

        self.bytes_available = self.bytes_available.saturating_sub(bytes as i128);

        if self.bytes_available < 0 {
            let wait_millis = self.bytes_available.saturating_mul(-1000) / max_bps;
            Some(std::time::Duration::from_millis(wait_millis as u64))
        } else {
            None
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
    fn should_accumulate_wait_if_called_too_often() {
        // This tests "going into debt".
        let mut limiter = LeakyBucket::new(100);
        assert_eq!(
            limiter.bytes_consumed_inner(110, 1000),
            Some(std::time::Duration::from_millis(100))
        );
        assert_eq!(
            limiter.bytes_consumed_inner(10, 0),
            Some(std::time::Duration::from_millis(200))
        );
        assert_eq!(
            limiter.bytes_consumed_inner(10, 0),
            Some(std::time::Duration::from_millis(300))
        );
    }

    #[test]
    fn should_wait_if_we_start_consuming_too_fast() {
        let mut limiter = LeakyBucket::new(100);

        // If we stay under the limit, should be fine.
        assert_eq!(limiter.bytes_consumed_inner(100, 1000), None);

        // If we go over the limit, we should be told to wait.
        assert_eq!(
            limiter.bytes_consumed_inner(110, 1000),
            Some(std::time::Duration::from_millis(100))
        );

        // If we wait, should go back to 0.
        assert_eq!(limiter.bytes_consumed_inner(0, 100), None);

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

        let mut limiter = LeakyBucket::new(100);
        assert_eq!(limiter.bytes_consumed_inner(100, u128::MAX), None);

        // This is going to return something very large and nonsensical, but
        // it shouldn't overflow.
        let mut limiter = LeakyBucket::new(100);
        assert!(
            limiter
                .bytes_consumed_inner(u64::MAX, u128::MAX)
                .unwrap()
                .as_secs()
                > 1_000_000
        );
    }
}
