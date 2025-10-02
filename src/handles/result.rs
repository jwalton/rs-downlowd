use std::path::PathBuf;
/// The result of a download operation.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct DownloadResult {
    pub tries: u64,
    pub path: PathBuf,
    pub bytes_downloaded: u64,
}
