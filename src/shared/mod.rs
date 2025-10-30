mod download;
mod download_config;
mod head;

pub(crate) use download::DownloadInner;
pub(crate) use download_config::{DownloadConfig, config_proxy};
pub(crate) use head::LazyHead;
