use std::sync::Arc;

use arc_swap::ArcSwap;
use tokio::select;

use crate::limiter::TokenBucket;

// Default rate limit. This is only ever set when the limiter is disabled.
const DEFAULT_MAX_BYTES_PER_SECOND: u64 = 1024 * 1024 * 1024; // 1 GB/s

pub struct TokioLimiter {
    max_bytes_per_second: ArcSwap<Option<u64>>,
    notify: tokio::sync::Notify,
    limiter: tokio::sync::Mutex<TokenBucket>,
}

impl TokioLimiter {
    /// Create a new TokioLimiter with the given maximum bytes per second.
    pub fn new(max_bytes_per_second: Option<u64>) -> Self {
        let bucket = TokenBucket::new(max_bytes_per_second.unwrap_or(DEFAULT_MAX_BYTES_PER_SECOND)); // Default to 1 MB/s

        TokioLimiter {
            max_bytes_per_second: ArcSwap::from_pointee(max_bytes_per_second),
            notify: tokio::sync::Notify::new(),
            limiter: tokio::sync::Mutex::new(bucket),
        }
    }

    /// Update the maximum bytes per second that can be downloaded.
    pub fn set_max_bytes_per_second(&self, max_bytes_per_second: Option<u64>) {
        self.max_bytes_per_second
            .swap(Arc::new(max_bytes_per_second));

        // Notify any waiters that the limit has changed.
        self.notify.notify_one();
    }

    /// Call this method when bytes have been consumed. If we try to consume bytes
    /// faster than is allowed, this will asynchronously wait until the bytes can
    /// be consumed, which will apply backpressure to the download.
    ///
    /// Returns the time spent waiting.
    pub async fn bytes_consumed(&self, bytes: u64, complete: bool) {
        if let Some(max_bps) = self.max_bytes_per_second.load().as_ref() {
            let mut limiter = self.limiter.lock().await;
            limiter.max_bytes_per_second = *max_bps;
            limiter.bytes_consumed(bytes);

            // If there's still bytes left to download, then wait for the limiter
            // to tell us we can continue, but if we've already downloaded the
            // whole file, there's not much point waiting around.
            if !complete {
                while let Some(delay) = limiter.time_to_wait() {
                    println!("jwalton - Sleeping for {delay:?}");
                    select! {
                        _ = tokio::time::sleep(delay) => {}
                        _ = self.notify.notified() => {
                            match self.max_bytes_per_second.load().as_ref() {
                                Some(max_bps) => {
                                    limiter.max_bytes_per_second = *max_bps;
                                }
                                None => {
                                    break;
                                }
                            }
                            println!("jwalton - Woke early");
                        }
                    };
                }
            }
        }
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
