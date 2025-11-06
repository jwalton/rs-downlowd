use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use tokio::select;

use crate::{limiter::{TokenBucket, UNLIMITED}, maybe_async};

pub struct TokioTokenBucket {
    ever_enabled: AtomicBool,
    max_bytes_per_second: AtomicU64,
    notify: tokio::sync::Notify,
    tokens: tokio::sync::Mutex<TokenBucket>,
}

impl TokioTokenBucket {
    /// Create a new TokioLimiter with the given maximum bytes per second.
    pub fn new(max_bytes_per_second: Option<u64>) -> Self {
        let bucket = TokenBucket::new();

        TokioTokenBucket {
            ever_enabled: AtomicBool::new(max_bytes_per_second.is_some()),
            max_bytes_per_second: AtomicU64::new(max_bytes_per_second.unwrap_or(UNLIMITED)),
            notify: tokio::sync::Notify::new(),
            tokens: tokio::sync::Mutex::new(bucket),
        }
    }

    /// Update the maximum bytes per second that can be downloaded.
    pub fn set_max_bytes_per_second(&self, max_bps: Option<u64>) {
        self.ever_enabled.fetch_or(max_bps.is_some(),  Ordering::Relaxed);
        self.max_bytes_per_second
            .store(max_bps.unwrap_or(0), Ordering::Relaxed);

        // Notify any waiters that the limit has changed.
        self.notify.notify_one();
    }

    /// Called to notify the bucket that we consumed some bytes.
    pub async fn bytes_consumed(&self, bytes: u64) {
        // TODO: Can we avoid taking the lock on `bytes_consumed`?
        let mut guard = self.tokens.lock().await;
        guard.bytes_consumed(bytes);
    }

    /// Called to wait until the caller can download more bytes.
    pub async fn wait(&self) {
        // If we've never turned on the limiter, bypass it.
        if !self.ever_enabled.load(Ordering::Relaxed) {
            return;
        }

        let mut guard = self.tokens.lock().await;
        while let Some(delay) =
            guard.time_to_wait(self.max_bytes_per_second.load(Ordering::Relaxed))
        {
            select! {
                _ = tokio::time::sleep(delay) => {}
                _ = self.notify.notified() => {
                    continue;
                }
            };
        }
    }
}

impl maybe_async::Limiter for TokioTokenBucket {
    async fn bytes_consumed(&self, bytes: u64) {
        self.bytes_consumed(bytes).await;
    }

    async fn wait(&self) {
        self.wait().await;
    }
}


#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    #[tokio::test]
    async fn should_not_wait_if_consuming_slow_enough() {
        let limiter = TokioTokenBucket::new(Some(1_000_000));
        let start = std::time::Instant::now();
        limiter.bytes_consumed(50).await;
        limiter.wait().await;
        limiter.bytes_consumed(50).await;
        limiter.wait().await;
        let elapsed = start.elapsed();
        assert!(elapsed.as_millis() < 10, "Elapsed time was {elapsed:?}");
    }

    #[tokio::test]
    async fn should_wait_if_consuming_too_fast() {
        let limiter = TokioTokenBucket::new(Some(100));
        let start = std::time::Instant::now();
        limiter.bytes_consumed(10).await;
        limiter.wait().await;
        let elapsed = start.elapsed();

        // This should have taken about 100ms.
        assert!(
            elapsed >= Duration::from_millis(90),
            "Elapsed time was {elapsed:?}, expected at least 100ms"
        );
    }
}
