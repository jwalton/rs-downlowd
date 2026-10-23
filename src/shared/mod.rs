//! Shared code common between our blocking and async implementations.
//!
//! This code makes heavy use of the traits defined in `crate::maybe_async`.
//! See the description there, but note that functions in this module should
//! delegate any IO to one of the traits from that module.
//!

mod download;
mod download_config;
mod head;

pub(crate) use download::DownloadInner;
pub(crate) use download_config::{DownloadConfig, config_proxy};
pub(crate) use head::LazyHead;
