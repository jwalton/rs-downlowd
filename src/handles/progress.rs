// FIXME: Add a "progress struct" here.

use std::path::{Path, PathBuf};

use url::Url;

pub struct ProgressHandle {
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
    /// Cached information about the local file, either from reading the sidecar
    /// file, or fetched from the server.
    pub(crate) local_file_info: crate::FileInfo,
    /// True if the download has been cancelled.
    pub(crate) cancelled: bool,
}

/// A trait for reporting progress of a download.
pub trait Progress {
    fn progress(&mut self, data: &mut ProgressHandle);
}

impl<T> Progress for T
where
    T: FnMut(&mut ProgressHandle),
{
    fn progress(&mut self, data: &mut ProgressHandle) {
        self(data);
    }
}

impl Progress for Box<dyn Progress> {
    fn progress(&mut self, data: &mut ProgressHandle) {
        self.as_mut().progress(data);
    }
}

impl ProgressHandle {
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

    /// Returns the size of the local file on disk, including any bytes downloaded
    /// in a previous partial download.
    pub fn bytes(&self) -> u64 {
        self.bytes
    }

    /// Returns the size of the file on the server, if known.
    pub fn total_bytes(&self) -> Option<u64> {
        self.local_file_info.file_length
    }

    /// Cancel this download. This will cause the download to stop immedaitely.
    /// Any partially downloaded file will be left on disk.
    pub fn cancel(&mut self) {
        self.cancelled = true;
    }

    /// Return the etag for the file, if known.
    pub fn etag(&self) -> Option<&str> {
        self.local_file_info.etag.as_deref()
    }

    /// Return the last modified time for the file, if known.
    pub fn last_modified(&self) -> Option<&str> {
        self.local_file_info.last_modified.as_deref()
    }
}
