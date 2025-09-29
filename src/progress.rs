// FIXME: Add a "progress struct" here.

use std::{
    path::{Path, PathBuf},
    time::Duration,
};

use url::Url;

use crate::Error;

pub enum ProgressEvent {
    /// We've made some progress downloading the file.
    BytesDownloaded,
    /// We've encountered an error, and will retry.
    // TODO: When will we retry?
    Err {
        err: Error,
        time_until_retry: Duration,
    },
    /// We're done downloading the file.  The file has been renamed to its final
    /// destination.
    Done,
}

pub struct ProgressData {
    /// The type of event we are reporting.
    pub(crate) event: ProgressEvent,
    /// The original URL we are downloading from.
    pub(crate) original_url: Url,
    /// The URL we are downloading from.  Note that if we followed a redirect,
    /// this may be different from the original URL.
    pub(crate) updated_url: Option<Url>,
    /// The final path we are downloading to.
    pub(crate) destination: PathBuf,
    /// The total number of tries so far to download this file.
    pub(crate) tries: u64,
    /// The actual number of bytes actually transfered so far.
    pub(crate) bytes_transferred: u64,
    /// The size of the local file on disk, including any bytes downloaded.
    pub(crate) bytes: u64,
    /// Total bytes in the file, if known.
    pub(crate) total_bytes: Option<u64>,
    /// True if the download has been cancelled.
    pub(crate) cancelled: bool,
}

/// A trait for reporting progress of a download.
pub trait Progress {
    fn progress(&mut self, data: &mut ProgressData);
}

impl<T> Progress for T
where
    T: FnMut(&mut ProgressData),
{
    fn progress(&mut self, data: &mut ProgressData) {
        self(data);
    }
}

impl Progress for Box<dyn Progress> {
    fn progress(&mut self, data: &mut ProgressData) {
        self.as_mut().progress(data);
    }
}

impl ProgressData {
    /// Returns the type of event we are reporting.
    pub fn event(&self) -> &ProgressEvent {
        &self.event
    }

    /// Returns the original URL we are downloading from.
    pub fn original_url(&self) -> &Url {
        &self.original_url
    }

    /// Returns the URL we are downloading from.  Note that if we followed a
    /// redirect, this may be different from the original URL.  If you are
    /// looking for the URL that was supplied by the user, see `original_url()`.
    pub fn url(&self) -> &Url {
        self.updated_url.as_ref().unwrap_or(&self.original_url)
    }

    /// Returns the final path we are downloading to.
    pub fn destination(&self) -> &Path {
        &self.destination
    }

    /// Returns the total number of tries so far to download this file.  Note that
    /// the retry count is reset every time we make progress downloading the file,
    /// so this number may be higher than the maximum number of retries allowed.
    pub fn tries(&self) -> u64 {
        self.tries
    }

    /// Return the actual number of bytes actually transfered so far.  If we are resuming
    /// a partial download, this will be less than the value returned by `bytes()`.
    /// If the file changes on the server and we have to restart the download,
    /// this could end up being greater than `total_bytes()`.
    pub fn bytes_transferred(&self) -> u64 {
        self.bytes_transferred
    }

    /// Returns the size of the local file on disk, including any bytes downloaded
    /// in a previous partial download.
    pub fn bytes(&self) -> u64 {
        self.bytes
    }

    /// Returns the total number of bytes in the file, if known.
    pub fn total_bytes(&self) -> Option<u64> {
        self.total_bytes
    }

    /// Cancel this download. This will cause the download to stop immedaitely.
    /// Any partially downloaded file will be left on disk.
    pub fn cancel(&mut self) {
        self.cancelled = true;
    }
}
