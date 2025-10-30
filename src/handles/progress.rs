use std::path::Path;

use http::Uri;

use crate::{Error, destination::Destination, file_info::FileInfo};

pub struct ProgressHandle {
    /// The original URL we are downloading from.
    pub(crate) original_uri: Uri,
    /// The URL we are downloading from.  Note that if we followed a redirect,
    /// this may be different from the original URL.
    pub(crate) updated_uri: Option<Uri>,
    /// The final path we are downloading to.
    pub(crate) destination: Destination,
    /// The total number of tries so far to download this file.
    pub(crate) tries: u64,
    /// The actual number of bytes actually transfered so far.
    pub(crate) bytes_transferred: u64,
    /// The size of the local file on disk, including any bytes downloaded.
    pub(crate) bytes: u64,
    /// The number of bytes transferred since the last time the progress
    /// handler was called.
    pub(crate) delta: u64,
    /// Cached information about the downloaded copy of the file, either from
    /// reading the sidecar file, or fetched from the server.
    pub(crate) local_file_info: FileInfo,
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
    #[must_use]
    pub(crate) fn new(
        original_uri: Uri,
        updated_uri: Option<Uri>,
        destination: Destination,
        local_file_info: FileInfo,
        existing_file_length: u64,
    ) -> Self {
        Self {
            original_uri,
            updated_uri,
            destination,
            tries: 0,
            bytes_transferred: 0,
            bytes: existing_file_length,
            delta: 0,
            local_file_info,
            cancelled: false,
        }
    }

    /// Returns true if we have the entire file already downloaded.
    #[must_use]
    pub(crate) fn is_complete(&self) -> Option<bool> {
        self.local_file_info
            .file_length
            .map(|len| self.bytes == len)
    }

    /// Notify that we've written some bytes.
    pub(crate) fn notify_bytes_written(
        &mut self,
        progress_handler: &mut Option<Box<dyn Progress + Send>>,
        bytes: u64,
    ) -> Result<(), Error> {
        self.delta = bytes;
        self.bytes += bytes;
        self.bytes_transferred += bytes;

        if let Some(handler) = progress_handler.as_mut() {
            handler.progress(self);
        }

        if self.cancelled {
            return Err(Error::Cancelled);
        }

        Ok(())
    }

    /// Notify the given progress handler with this progress object.
    pub(crate) fn notify(
        &mut self,
        progress_handler: &mut Option<Box<dyn Progress + Send>>,
    ) -> Result<(), Error> {
        if let Some(handler) = progress_handler.as_mut() {
            handler.progress(self);
        }

        if self.cancelled {
            return Err(Error::Cancelled);
        }

        Ok(())
    }

    /// Returns the original URL we are downloading from.
    #[must_use]
    pub fn original_uri(&self) -> &Uri {
        &self.original_uri
    }

    /// Returns the URL we are downloading from.  Note that if we followed a
    /// redirect, this may be different from the original URL.  If you are
    /// looking for the URL that was supplied by the user, see `original_url()`.
    #[must_use]
    pub fn uri(&self) -> &Uri {
        self.updated_uri.as_ref().unwrap_or(&self.original_uri)
    }

    /// Returns the final path we are downloading to.
    #[must_use]
    pub fn destination(&self) -> &Path {
        &self.destination.path
    }

    /// Returns the total number of tries so far to download this file.  Note that
    /// the retry count is reset every time we make progress downloading the file,
    /// so this number may be higher than the maximum number of retries allowed.
    #[must_use]
    pub fn tries(&self) -> u64 {
        self.tries
    }

    /// Returns the number of bytes transferred since the last time the progress
    /// handler was called.
    #[must_use]
    pub fn delta(&self) -> u64 {
        self.delta
    }

    /// Returns the size of the local file on disk, including any bytes downloaded
    /// in a previous partial download.
    #[must_use]
    pub fn bytes(&self) -> u64 {
        self.bytes
    }

    /// Returns the size of the file on the server, if known.
    #[must_use]
    pub fn remote_length(&self) -> Option<u64> {
        self.local_file_info.file_length
    }

    /// Cancel this download. This will cause the download to stop immedaitely.
    /// Any partially downloaded file will be left on disk.
    pub fn cancel(&mut self) {
        self.cancelled = true;
    }

    /// Return the etag for the file, if known.
    #[must_use]
    pub fn etag(&self) -> Option<&str> {
        self.local_file_info.etag.as_deref()
    }

    /// Return the last modified time for the file, if known.
    #[must_use]
    pub fn last_modified(&self) -> Option<&str> {
        self.local_file_info.last_modified.as_deref()
    }
}
