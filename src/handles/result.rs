use std::path::PathBuf;

/// The result of a download operation.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct DownloadResult {
    /// The total number of tries we made to download the file.
    pub tries: u64,
    /// The path the file was saved to.
    pub path: PathBuf,
    /// The total number of bytes downloaded.
    pub bytes_downloaded: u64,
}
