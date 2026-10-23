//! This module implements types like `Progress` and `ProgressHandle`, used for
//! reporting progress, or `RetryHandler` and `RetryHandle`, used to determine
//! whether or not we should retry a download.

mod progress;
mod result;
mod retry;

pub use progress::{Progress, ProgressHandle};
pub use result::{DownloadResult};
pub use retry::{RetryHandle, RetryHandler, default_retry_callback};
