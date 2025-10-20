use std::path::PathBuf;

use crate::ProgressHandle;

/// The result of a download operation.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct DownloadResult {
    /// The total number of tries we made to download the file.
    pub tries: u64,
    /// The path the file was saved to.
    pub path: PathBuf,
    /// The size of the downloaded file.
    pub file_size: u64,
    /// The total number of bytes downloaded.  This may be less than the file size
    /// if the download was resumed.
    pub bytes_downloaded: u64,
}

impl DownloadResult {
    /// Create a new DownloadResult.
    #[must_use]
    pub(crate) fn new(progress: ProgressHandle) -> Self {
        Self {
            tries: progress.tries,
            path: progress.destination.path,
            file_size: progress.bytes,
            bytes_downloaded: progress.bytes_transferred,
        }
    }
}
