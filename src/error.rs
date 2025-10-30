use std::path::PathBuf;

use thiserror::Error;

/// Errors from the downlowd crate.  Some errors are "retryable", meaning that
/// downlowd will automatically retry the download if the error occurs. Retryable
/// errors are marked as such.
#[derive(Error, Debug)]
#[non_exhaustive]
pub enum Error {
    /// The configuration provided was invalid.
    #[error("Invalid configuration: {message}")]
    #[non_exhaustive]
    InvalidConfig { message: String },

    /// The URL provided was invalid.
    #[error("Invalid URL: {cause}")]
    #[non_exhaustive]
    InvalidUrl { cause: String },

    /// The caller attempted to add a header to the download, but the header contained invalid characters.
    #[error("Invalid header: {cause}")]
    #[non_exhaustive]
    InvalidHeader { cause: String },

    /// There was a network error when downloading the file (e.g. the host is unreachable, the connection was reset, etc).
    /// Network errors are retryable.
    #[error("Network error for {uri}: {cause}")]
    #[non_exhaustive]
    Network { uri: String, cause: String },

    /// When attempting to download the file, the client was redirected too many times,
    /// was redirected in a loop, or was redirected to an invalid URL.
    #[error("Bad redirect: {reason}")]
    #[non_exhaustive]
    BadRedirect { reason: &'static str },

    /// There was an error writing the file to disk.
    #[error("Error {action} for {path}: {cause}")]
    #[non_exhaustive]
    Write {
        action: &'static str,
        path: PathBuf,
        cause: std::io::Error,
    },

    /// The server returned an error status code.  UnexpectedStatus errors may
    /// be retryable, depending on the status code.  5xx status codes are
    /// retryable, while 4xx status codes are not.
    #[error("Unexpected response status: {status}")]
    #[non_exhaustive]
    UnexpectedStatus { status: u16 },

    /// FileChanged is thown when attempting to resume a download, and the etag,
    /// last-modified, or file length returned by the server differs from what
    /// we expect.  FileChanged errors are retryable.
    #[error("File changed on server during download: {description}")]
    #[non_exhaustive]
    FileChanged { description: &'static str },

    /// The download was cancelled by the user.
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

    /// Return a copy of this error with the URL elided.
    pub fn without_uri(self) -> Self {
        if let Self::Network { cause, .. } = self {
            Self::Network {
                uri: "unknown".to_string(),
                cause,
            }
        } else {
            self
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
