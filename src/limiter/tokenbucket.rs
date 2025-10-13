use std::time::{Duration, Instant};

pub struct TokenBucket {
    /// The target maximum bytes per second.
    pub max_bytes_per_second: u64,

    /// The last time we updated the bucket.
    last: Instant,

    /// The number of tokens in the bucket.  We slowly drain the bucket
    /// over time, up to a maximum of `max_bytes_per_second`.
    tokens: u64,
}

impl TokenBucket {
    /// Create a new limiter that allows approximately `max_bytes_per_second`
    /// kilobytes per second to be downloaded.
    pub fn new(max_bytes_per_second: u64) -> Self {
        Self {
            max_bytes_per_second,
            last: Instant::now(),
            tokens: 0,
        }
    }

    /// Called to notify the bucket that we consumed some bytes.
    pub fn bytes_consumed(&mut self, bytes: u64) {
        self.tokens = self.tokens.saturating_add(bytes);
    }

    /// Called to find out how long the caller should wait before trying to
    /// download more bytes.  Note that if we're sharing this [`TokenBucket`]
    /// between multiple threads, and we release the lock on the bucket while we
    /// wait, it's possible other users of the bucket will consume more bytes,
    /// increasing the wait time again.
    pub fn time_to_wait(&mut self) -> Option<Duration> {
        // Work out how long it's been since someone called this.
        let now = Instant::now();
        let elapsed = now.duration_since(self.last);
        self.last = now;

        self.time_to_wait_inner(elapsed)
    }

    #[inline]
    fn time_to_wait_inner(&mut self, elapsed: Duration) -> Option<Duration> {
        let elapsed_ms = elapsed.as_millis();

        if elapsed_ms > u64::MAX as u128 {
            // If we've waited more than 550 million years, just reset the bucket.
            self.tokens = 0;
        } else {
            // Remove tokens from the bucket based on the elapsed time.
            let tokens_to_remove =
                (elapsed_ms as u64).saturating_mul(self.max_bytes_per_second) / 1000;
            self.tokens = self.tokens.saturating_sub(tokens_to_remove);
        }

        if self.tokens > 0 {
            // There's tokens left over - need to wait for them to clear.
            let wait_millis = self.tokens.saturating_mul(1000) / self.max_bytes_per_second;
            Some(Duration::from_millis(wait_millis))
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn consume_bytes(limiter: &mut TokenBucket, bytes: u64, ms: u64) -> Option<Duration> {
        limiter.bytes_consumed(bytes);
        limiter.time_to_wait_inner(Duration::from_millis(ms))
    }

    #[test]
    fn should_not_wait_if_consuming_slow_enough() {
        let mut limiter = TokenBucket::new(100);
        assert_eq!(consume_bytes(&mut limiter, 100, 1000), None);
        assert_eq!(consume_bytes(&mut limiter, 100, 1000), None);
        assert_eq!(consume_bytes(&mut limiter, 100, 1000), None);
        assert_eq!(consume_bytes(&mut limiter, 50, 1000), None);
        assert_eq!(consume_bytes(&mut limiter, 10, 100), None);
        assert_eq!(consume_bytes(&mut limiter, 10, 100), None);
        assert_eq!(consume_bytes(&mut limiter, 10, 100), None);
        assert_eq!(consume_bytes(&mut limiter, 0, 1000), None);
        assert_eq!(consume_bytes(&mut limiter, 0, 0), None);
    }

    #[test]
    fn should_accumulate_wait_if_called_too_often() {
        // This tests "going into debt".
        let mut limiter = TokenBucket::new(100);
        assert_eq!(
            consume_bytes(&mut limiter, 110, 1000),
            Some(Duration::from_millis(100))
        );
        assert_eq!(
            consume_bytes(&mut limiter, 10, 0),
            Some(Duration::from_millis(200))
        );
        assert_eq!(
            consume_bytes(&mut limiter, 10, 0),
            Some(Duration::from_millis(300))
        );
    }

    #[test]
    fn should_wait_if_we_start_consuming_too_fast() {
        let mut limiter = TokenBucket::new(100);

        // If we stay under the limit, should be fine.
        assert_eq!(consume_bytes(&mut limiter, 100, 1000), None);

        // If we go over the limit, we should be told to wait.
        assert_eq!(
            consume_bytes(&mut limiter, 110, 1000),
            Some(Duration::from_millis(100))
        );

        // If we wait, should go back to 0.
        assert_eq!(consume_bytes(&mut limiter, 0, 100), None);
    }

    #[test]
    fn should_work_for_big_numbers() {
        let mut limiter = TokenBucket::new(100);
        assert_eq!(
            consume_bytes(&mut limiter, 100_000_000, 0),
            Some(Duration::from_secs(1_000_000))
        );

        let mut limiter = TokenBucket::new(100);
        assert_eq!(consume_bytes(&mut limiter, 100, u64::MAX), None);

        // This is going to return something very large and nonsensical, but
        // it shouldn't overflow.
        let mut limiter = TokenBucket::new(100);
        assert!(
            consume_bytes(&mut limiter, u64::MAX, u64::MAX)
                .unwrap()
                .as_secs()
                > 1_000_000
        );
    }
}
