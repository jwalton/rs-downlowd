// FIXME: Add a "progress struct" here.

use std::path::Path;

use url::Url;

pub struct ProgressData<'a> {
    /// The original URL we are downloading from.
    pub(crate) original_url: &'a Url,
    /// The URL we are downloading from.  Note that if we followed a redirect,
    /// this may be different from the original URL.
    pub(crate) url: &'a Url,
    /// The final path we are downloading to.
    pub(crate) destination: &'a Path,
    /// The total number of tries so far to download this file.
    pub(crate) tries: u64,
    /// The actual number of bytes actually transfered so far.
    pub(crate) bytes_transferred: u64,
    /// The size of the local file on disk, including any bytes downloaded.
    pub(crate) bytes: u64,
    /// Total bytes in the file, if known.
    pub(crate) total_bytes: Option<u64>,
}

/// A trait for reporting progress of a download.
pub trait Progress {
    fn progress(&mut self, data: &ProgressData);
}

impl<T> Progress for T
where
    T: FnMut(&ProgressData),
{
    fn progress(&mut self, data: &ProgressData) {
        self(data);
    }
}

impl Progress for Box<dyn Progress> {
    fn progress(&mut self, data: &ProgressData) {
        self.as_mut().progress(data);
    }
}

impl ProgressData<'_> {
    /// Returns the original URL we are downloading from.
    pub fn original_url(&self) -> &Url {
        self.original_url
    }

    /// Returns the URL we are downloading from.  Note that if we followed a
    /// redirect, this may be different from the original URL.  If you are
    /// looking for the URL that was supplied by the user, see `original_url()`.
    pub fn url(&self) -> &Url {
        self.url
    }

    /// Returns the final path we are downloading to.
    pub fn destination(&self) -> &Path {
        self.destination
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
}
