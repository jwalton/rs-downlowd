// FIXME: Add a "progress struct" here.

use std::path::Path;

use url::Url;

pub struct ProgressData<'a> {
    pub url: &'a Url,
    pub destination: &'a Path,
    pub bytes_downloaded: u64,
    pub total_bytes: Option<u64>,
}

/// A trait for reporting progress of a download.
pub trait Progress {
    fn progress(&mut self, data: &ProgressData);
}

impl<T> Progress for T
where
    T: FnMut(u64, Option<u64>),
{
    fn progress(&mut self, data: &ProgressData) {
        self(data.bytes_downloaded, data.total_bytes);
    }
}
