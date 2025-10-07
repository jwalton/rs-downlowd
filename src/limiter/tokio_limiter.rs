use std::{sync::atomic::AtomicBool, time::Duration};

use tokio::select;

use crate::limiter::LeakyBucket;

// Default rate limit. This is only ever set when the limiter is disabled.
const DEFAULT_MAX_BYTES_PER_SECOND: u64 = 1024 * 1024 * 1024; // 1 GB/s

pub struct TokioLimiter {
    enabled: AtomicBool,
    broadcast: tokio::sync::broadcast::Sender<()>,
    limiter: std::sync::Mutex<LeakyBucket>,
}

impl TokioLimiter {
    /// Create a new TokioLimiter with the given maximum bytes per second.
    pub fn new(max_bytes_per_second: Option<u64>) -> Self {
        let enabled = max_bytes_per_second.is_some();
        let bucket = LeakyBucket::new(max_bytes_per_second.unwrap_or(DEFAULT_MAX_BYTES_PER_SECOND)); // Default to 1 MB/s

        TokioLimiter {
            enabled: AtomicBool::new(enabled),
            broadcast: tokio::sync::broadcast::channel(1).0,
            limiter: std::sync::Mutex::new(bucket),
        }
    }

    /// Update the maximum bytes per second that can be downloaded.
    pub async fn set_max_bytes_per_second(&self, max_bytes_per_second: Option<u64>) {
        {
            let mut limiter = self.limiter.lock().unwrap();
            limiter.max_bytes_per_second =
                max_bytes_per_second.unwrap_or(DEFAULT_MAX_BYTES_PER_SECOND);
        }

        let enabled = max_bytes_per_second.map(|v| v > 0).unwrap_or(false);

        self.enabled
            .store(enabled, std::sync::atomic::Ordering::Relaxed);

        // Notify any waiters that the limit has changed.
        _ = self.broadcast.send(());
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
    pub async fn bytes_consumed(&self, bytes: u64, complete: bool) {
        if self.is_enabled() {
            let mut receiver = self.broadcast.subscribe();
            let _ = self.get_delay(bytes);

            // If there's still bytes left to download, then wait for the limiter
            // to tell us we can continue, but if we've already downloaded the
            // whole file, there's not much point waiting around.
            if !complete {
                while let Some(delay) = self.get_delay(0) {
                    select! {
                        _ = tokio::time::sleep(delay) => {}
                        _ = receiver.recv() => {}
                    };
                }
            }
        }
    }

    fn get_delay(&self, bytes: u64) -> Option<Duration> {
        let mut limiter = self.limiter.lock().unwrap();
        limiter.bytes_consumed(bytes)
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
        limiter.bytes_consumed(50, false).await;
        limiter.bytes_consumed(50, false).await;
        let elapsed = start.elapsed();
        assert!(elapsed.as_millis() < 100, "Elapsed time was {elapsed:?}");
    }

    #[tokio::test]
    async fn should_wait_if_consuming_too_fast() {
        let limiter = TokioLimiter::new(Some(100));
        let start = std::time::Instant::now();
        limiter.bytes_consumed(10, false).await;
        let elapsed = start.elapsed();

        // This should have taken about 100ms.
        assert!(
            elapsed >= Duration::from_millis(90),
            "Elapsed time was {elapsed:?}, expected at least 100ms"
        );
    }

    #[tokio::test]
    async fn should_not_wait_if_consuming_too_fast_but_we_are_done() {
        let limiter = TokioLimiter::new(Some(100));
        let start = std::time::Instant::now();
        limiter.bytes_consumed(10, true).await;
        let elapsed = start.elapsed();

        // This should have taken about 100ms.
        assert!(
            elapsed < Duration::from_millis(10),
            "Elapsed time was {elapsed:?}, expected at least 100ms"
        );
    }
}
