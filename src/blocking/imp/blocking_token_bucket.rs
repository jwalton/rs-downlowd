use std::sync::{
    Condvar, Mutex,
    atomic::{AtomicBool, AtomicU64, Ordering},
};

use crate::{
    limiter::{TokenBucket, UNLIMITED},
    maybe_async::Limiter,
};

/// Thread safe token-bucket rate limiter.
pub struct BlockingTokenBucket {
    ever_enabled: AtomicBool,

    /// The target maximum bytes per second.
    pub max_bytes_per_second: AtomicU64,

    /// Condvar used to wake if the max_bytes_per_second changes.
    cond: Condvar,

    /// Tokens currently in the bucket.
    tokens: Mutex<TokenBucket>,
}

impl BlockingTokenBucket {
    /// Create a new limiter that allows approximately `max_bytes_per_second`
    /// kilobytes per second to be downloaded.
    pub fn new(max_bytes_per_second: Option<u64>) -> Self {
        Self {
            ever_enabled: AtomicBool::new(max_bytes_per_second.is_some()),
            max_bytes_per_second: AtomicU64::new(max_bytes_per_second.unwrap_or(UNLIMITED)),
            cond: Condvar::new(),
            tokens: Mutex::new(TokenBucket::new()),
        }
    }

    /// Called to notify the bucket that we consumed some bytes.
    pub fn bytes_consumed(&self, bytes: u64) {
        let mut guard = self.tokens.lock().unwrap();
        guard.bytes_consumed(bytes);
    }

    /// Set the maximum bytes per second for this TokenBucket instance.
    pub fn set_max_bytes_per_second(&self, max_bps: Option<u64>) {
        self.ever_enabled
            .fetch_or(max_bps.is_some(), Ordering::Relaxed);
        self.max_bytes_per_second
            .store(max_bps.unwrap_or(0), Ordering::Relaxed);

        if max_bps.is_none() {
            let mut tokens = self.tokens.lock().unwrap();
            tokens.clear();
        }

        // Wake up anyone waiting for tokens.
        self.cond.notify_all();
    }

    /// Called to wait until the caller can download more bytes.
    pub fn wait(&self) {
        // If we've never turned on the limiter, bypass it.
        if !self.ever_enabled.load(Ordering::Relaxed) {
            return;
        }

        let mut tokens = self.tokens.lock().unwrap();
        while let Some(delay) =
            tokens.time_to_wait(self.max_bytes_per_second.load(Ordering::Relaxed))
        {
            // This will wake if max_bytes_per_second changes, otherwise it'll
            // sleep until we're ready too download more bytes.
            let result = self.cond.wait_timeout(tokens, delay).unwrap();
            tokens = result.0;
        }
    }
}

impl Limiter for BlockingTokenBucket {
    async fn bytes_consumed(&self, bytes: u64) {
        self.bytes_consumed(bytes);
    }

    async fn wait(&self) {
        self.wait();
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    #[test]
    fn should_not_wait_if_consuming_slow_enough() {
        let limiter = BlockingTokenBucket::new(Some(1_000_000));
        let start = std::time::Instant::now();
        limiter.bytes_consumed(50);
        limiter.wait();
        limiter.bytes_consumed(50);
        limiter.wait();
        let elapsed = start.elapsed();
        assert!(elapsed.as_millis() < 10, "Elapsed time was {elapsed:?}");
    }

    #[test]
    fn should_wait_if_consuming_too_fast() {
        let limiter = BlockingTokenBucket::new(Some(100));
        let start = std::time::Instant::now();
        limiter.bytes_consumed(10);
        limiter.wait();
        let elapsed = start.elapsed();

        // This should have taken about 100ms.
        assert!(
            elapsed >= Duration::from_millis(90),
            "Elapsed time was {elapsed:?}, expected at least 100ms"
        );
    }
}
