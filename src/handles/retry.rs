use std::time::Duration;

use crate::{Error, DEFAULT_MAX_DELAY, DEFAULT_MIN_DELAY};

/// Callback function to invoke when trying a download.
pub type RetryHandler = Box<dyn FnMut(&mut RetryHandle) + Send>;

/// Handle passed to the `on_retry` callback.
pub struct RetryHandle {
    /// The total number of tries so far to download this file.
    pub(crate) total_tries: u64,
    /// The number of consecutive retries due to errors.
    pub(crate) retries: u64,
    /// The delay before the next retry.
    pub(crate) delay: std::time::Duration,
    /// The error that caused the retry.
    pub(crate) error: crate::Error,
    /// True if the download has been cancelled.
    pub(crate) cancelled: bool,
}

impl RetryHandle {
    pub fn new(
        total_tries: u64,
        retries: u64,
        delay: std::time::Duration,
        error: crate::Error,
    ) -> Self {
        Self {
            total_tries,
            retries,
            delay,
            error,
            cancelled: false,
        }
    }

    /// Returns the total number of tries so far to download this file.
    pub fn total_tries(&self) -> u64 {
        self.total_tries
    }

    /// Returns the number of consecutive retries due to errors.  This number
    /// resets to zero if we make progress downloading a file.
    pub fn retries(&self) -> u64 {
        self.retries
    }

    /// Returns the delay before the next retry.
    pub fn delay(&self) -> std::time::Duration {
        self.delay
    }

    /// Sets the delay before the next retry.
    pub fn set_delay(&mut self, delay: std::time::Duration) {
        self.delay = delay;
    }

    /// Returns the error that caused the retry.
    pub fn error(&self) -> &crate::Error {
        &self.error
    }

    /// Cancels the download.  This will cause the download to fail the error
    /// that caused the retry.
    pub fn cancel(&mut self) {
        self.cancelled = true;
    }
}

/// Default callback used for determining backoff delay between retries.
pub fn default_retry_callback(handle: &mut RetryHandle) {
    if matches!(handle.error(), Error::FileChanged { .. }) {
        // No delay if the file changed.
        handle.set_delay(Duration::ZERO);
    } else {
        handle.set_delay(crate::exponential_backoff(
            DEFAULT_MIN_DELAY,
            DEFAULT_MAX_DELAY,
            handle.retries(),
        ));
    }
}
