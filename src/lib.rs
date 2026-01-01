#![cfg_attr(docsrs, feature(doc_auto_cfg))]

#![doc = include_str!("../README.md")]

mod backoff;
mod client_builder;
mod destination;
mod error;
mod file_info;
mod handles;
mod headers;
mod limiter;
mod maybe_async;
mod shared;
mod utils;

use std::time::Duration;

pub use backoff::exponential_backoff;
pub use error::Error;
pub use handles::{DownloadResult, Progress, ProgressHandle, RetryHandle, RetryHandler};
pub use http::{HeaderMap, HeaderValue, header::IntoHeaderName};
pub use utils::into_uri::IntoUri;

#[cfg(feature = "blocking")]
pub mod blocking;

#[cfg(feature = "async")]
mod nonblocking;
#[cfg(feature = "async")]
pub use client_builder::ClientBuilder;
#[cfg(feature = "async")]
pub use nonblocking::{Client, Download};

/// Default number of retries for a download.
const DEFAULT_MAX_RETRIES: Option<u64> = Some(5);

/// Default minimum delay between retries.
const DEFAULT_MIN_DELAY: Duration = Duration::from_secs(1);

/// Default maximum delay between retries.
const DEFAULT_MAX_DELAY: Duration = Duration::from_secs(120);
