use std::path::PathBuf;

use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("Network error during {during} for {url}: {cause}")]
    Network {
        during: &'static str,
        url: String,
        cause: reqwest::Error,
    },

    #[error("Error {action} for {path}: {cause}")]
    Write {
        action: &'static str,
        path: PathBuf,
        cause: std::io::Error,
    },

    #[error("Unexpected response status: {0}")]
    UnexpectedStatus(u16),

    #[error("File changed on server during download")]
    FileChanged { description: &'static str },
}

impl Error {
    /// Return true if this is an error that can be retried.
    pub(crate) fn can_retry(&self) -> bool {
        matches!(self, Error::Network { .. } | Error::FileChanged { .. })
    }
}
