use std::path::PathBuf;

use thiserror::Error;

#[derive(Error, Debug)]
#[non_exhaustive]
pub enum Error {
    #[error("Invalid configuration: {message}")]
    #[non_exhaustive]
    InvalidConfig { message: String },

    #[error("Invalid URL: {cause}")]
    #[non_exhaustive]
    InvalidUrl { cause: String },

    #[error("Invalid header: {cause}")]
    #[non_exhaustive]
    InvalidHeader { cause: String },

    #[error("Network error during {during} for {url}: {cause}")]
    #[non_exhaustive]
    Network {
        during: &'static str,
        url: String,
        cause: reqwest::Error,
    },

    #[error("Bad redirect: {reason}")]
    #[non_exhaustive]
    BadRedirect { reason: &'static str },

    #[error("Error {action} for {path}: {cause}")]
    #[non_exhaustive]
    Write {
        action: &'static str,
        path: PathBuf,
        cause: std::io::Error,
    },

    #[error("Unexpected response status: {status}")]
    #[non_exhaustive]
    UnexpectedStatus { status: u16 },

    #[error("File changed on server during download")]
    #[non_exhaustive]
    FileChanged { description: &'static str },

    #[error("Download was cancelled")]
    Cancelled,
}

impl Error {
    /// Return true if this is an error that can be retried.
    pub(crate) fn can_retry(&self) -> bool {
        match self {
            Error::Network { .. } | Error::FileChanged { .. } => true,
            Error::UnexpectedStatus { status } => {
                // 400 errors are not retryable.
                *status < 400 || *status >= 500
            }
            Error::InvalidConfig { .. }
            | Error::InvalidUrl { .. }
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

        assert!(Error::UnexpectedStatus { status: 500 }.can_retry());

        assert!(!Error::UnexpectedStatus { status: 404 }.can_retry());
    }
}
