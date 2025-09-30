use std::{cmp::min, time::Duration};

/// Function which calculates an exponential backoff duration based on the number of failures.
pub fn exponential_backoff(
    initial_delay: Duration,
    max_delay: Duration,
    failures: u64,
) -> Duration {
    if failures == 0 {
        Duration::ZERO
    } else {
        let failures = min(failures, u32::MAX as u64) as u32;
        (initial_delay * 2u32.saturating_pow(failures - 1)).min(max_delay)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn should_backoff() {
        let backoff = |failures| {
            exponential_backoff(
                Duration::from_millis(100),
                Duration::from_secs(10),
                failures,
            )
        };

        assert_eq!(backoff(0), Duration::ZERO);
        assert_eq!(backoff(1), Duration::from_millis(100));
        assert_eq!(backoff(2), Duration::from_millis(200));
        assert_eq!(backoff(3), Duration::from_millis(400));
        assert_eq!(backoff(4), Duration::from_millis(800));
        assert_eq!(backoff(5), Duration::from_millis(1600));
        assert_eq!(backoff(6), Duration::from_millis(3200));
        assert_eq!(backoff(7), Duration::from_millis(6400));
        assert_eq!(backoff(8), Duration::from_millis(10000));
        assert_eq!(backoff(9), Duration::from_millis(10000));
        assert_eq!(backoff(u32::MAX as u64), Duration::from_millis(10000));
        assert_eq!(backoff(u32::MAX as u64 + 1), Duration::from_millis(10000));
        assert_eq!(backoff(u64::MAX), Duration::from_millis(10000));
    }
}
