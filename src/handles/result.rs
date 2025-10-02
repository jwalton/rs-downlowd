use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Status {
    /// The file was successfully downloaded.
    Downloaded,
    /// The file was skipped, because a file with the same name and size already exists.
    Skipped,
}

/// The result of a download operation.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct DownloadResult {
    pub status: Status,
    pub tries: u64,
    pub path: PathBuf,
    pub bytes_downloaded: u64,
}
