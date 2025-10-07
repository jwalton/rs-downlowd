#![doc = include_str!("../README.md")]

mod backoff;
mod client;
mod destination;
mod download;
mod error;
mod file_info;
mod handles;
mod head;
mod headers;
mod limiter;
mod utils;

#[cfg(test)]
mod tests;

use std::time::Duration;

pub use backoff::exponential_backoff;
pub use client::{Client, ClientBuilder};
pub use download::Download;
pub use error::Error;
pub use handles::*;
pub use http::{HeaderMap, HeaderValue, header::IntoHeaderName};
pub use utils::into_url::IntoUrl;

/// Default number of retries for a download.
const DEFAULT_MAX_RETRIES: Option<u64> = Some(5);

/// Default minimum delay between retries.
const DEFAULT_MIN_DELAY: Duration = Duration::from_secs(1);

/// Default maximum delay between retries.
const DEFAULT_MAX_DELAY: Duration = Duration::from_secs(120);
