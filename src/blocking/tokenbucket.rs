use std::{
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering}, Mutex
    },
    thread,
    time::Duration,
};

use crate::limiter::{TokenBucket, UNLIMITED};

/// Thread safe token-bucket rate limiter.
pub struct BlockingTokenBucket {
    ever_enabled: AtomicBool,

    /// The target maximum bytes per second.
    pub max_bytes_per_second: AtomicU64,

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
            // Don't sleep for more than 100ms at a time.  If someone sets the
            // `max_bytes_per_second` to a really small value, we could end up
            // having to wait a long time here, but if they change it back, we
            // want to respond to that change quickly.
            let delay = delay.min(Duration::from_millis(100));
            thread::sleep(delay);
        }
    }
}
