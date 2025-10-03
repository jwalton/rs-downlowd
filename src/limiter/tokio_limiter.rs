use std::{sync::atomic::AtomicBool, time::Duration};

use crate::limiter::LeakyBucket;

// Default rate limit. This is only ever set when the limiter is disabled.
const DEFAULT_MAX_BYTES_PER_SECOND: u64 = 1024 * 1024 * 1024; // 1 GB/s

pub struct TokioLimiter {
    enabled: AtomicBool,
    limiter: tokio::sync::Mutex<LeakyBucket>,
}

impl TokioLimiter {
    /// Create a new TokioLimiter with the given maximum bytes per second.
    pub fn new(max_bytes_per_second: Option<u64>) -> Self {
        let enabled = max_bytes_per_second.is_some();
        let bucket = LeakyBucket::new(max_bytes_per_second.unwrap_or(DEFAULT_MAX_BYTES_PER_SECOND)); // Default to 1 MB/s

        TokioLimiter {
            enabled: AtomicBool::new(enabled),
            limiter: tokio::sync::Mutex::new(bucket),
        }
    }

    /// Update the maximum bytes per second that can be downloaded.
    pub async fn set_max_bytes_per_second(&self, max_bytes_per_second: Option<u64>) {
        // TODO: Would be nice if we didn't have to wait for the lock here.
        // Use an atomic for max_bytes_per_second?  Set this "in the background"?
        let mut limiter = self.limiter.lock().await;
        limiter.max_bytes_per_second = max_bytes_per_second.unwrap_or(DEFAULT_MAX_BYTES_PER_SECOND);
        self.enabled.store(
            max_bytes_per_second.is_some() && limiter.max_bytes_per_second > 0,
            std::sync::atomic::Ordering::Relaxed,
        );
    }

    /// Return true if the limiter is enabled.
    fn is_enabled(&self) -> bool {
        self.enabled.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Call this method when bytes have been consumed. If we try to consume bytes
    /// faster than is allowed, this will asynchronously wait until the bytes can
    /// be consumed, which will apply backpressure to the download.
    ///
    /// Returns the time spent waiting.
    pub async fn bytes_consumed(&self, bytes: u64) -> Duration {
        let mut waited = Duration::ZERO;

        if self.is_enabled() {
            // Split up calls into the rate limiter into chunks  to avoid long waits.
            // This makes the download more responsive to changes in the rate limit,
            // and to cancellation.
            let mut bytes_left = bytes;
            while bytes_left > 0 {
                let mut limiter = self.limiter.lock().await;

                let chunk = bytes_left.min(limiter.max_bytes_per_second);
                bytes_left -= chunk;

                if let Some(delay) = limiter.bytes_consumed(chunk) {
                    waited = waited.saturating_add(delay);
                    tokio::time::sleep(delay).await;
                }
            }
        }

        waited
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    #[tokio::test]
    async fn should_not_wait_if_consuming_slow_enough() {
        let limiter = TokioLimiter::new(Some(1_000_000));
        let start = std::time::Instant::now();
        limiter.bytes_consumed(50).await;
        limiter.bytes_consumed(50).await;
        let elapsed = start.elapsed();
        assert!(elapsed.as_millis() < 100, "Elapsed time was {elapsed:?}");
    }

    #[tokio::test]
    async fn should_wait_if_consuming_too_fast() {
        let limiter = TokioLimiter::new(Some(100));
        let start = std::time::Instant::now();
        limiter.bytes_consumed(10).await;
        let elapsed = start.elapsed();

        // This should have taken about 100ms.
        assert!(
            elapsed >= Duration::from_millis(90),
            "Elapsed time was {elapsed:?}, expected at least 100ms"
        );
    }
}
