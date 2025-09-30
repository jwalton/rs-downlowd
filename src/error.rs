use std::path::PathBuf;

use thiserror::Error;

#[derive(Error, Debug)]
#[non_exhaustive]
pub enum Error {
    #[error("Invalid URL: {cause}")]
    InvalidUrl { cause: String },

    #[error("Invalid header: {cause}")]
    InvalidHeader { cause: String },

    #[error("Network error during {during} for {url}: {cause}")]
    Network {
        during: &'static str,
        url: String,
        cause: reqwest::Error,
    },

    #[error("Bad redirect: {reason}")]
    BadRedirect { reason: &'static str },

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

    #[error("Download was cancelled")]
    Cancelled,
}

impl Error {
    /// Return true if this is an error that can be retried.
    pub(crate) fn can_retry(&self) -> bool {
        match self {
            Error::Network { .. } | Error::FileChanged { .. } => true,
            Error::UnexpectedStatus(status) => {
                // 400 errors are not retryable.
                *status < 400 || *status >= 500
            }
            Error::InvalidUrl { .. }
            | Error::InvalidHeader { .. }
            | Error::Write { .. }
            | Error::BadRedirect { .. }
            | Error::Cancelled => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Error;

    #[test]
    fn test_can_retry() {
        assert!(
            Error::FileChanged {
                description: "etag changed"
            }
            .can_retry()
        );

        assert!(Error::UnexpectedStatus(500).can_retry());

        assert!(!Error::UnexpectedStatus(404).can_retry());
    }
}
