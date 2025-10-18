use std::borrow::Cow;

use http::{HeaderMap, StatusCode};
use url::Url;

use crate::{Error, file_info::FileInfo};

/// Information about a URL fetched with a HEAD request.
#[derive(Debug, Default)]
pub struct Head {
    pub updated_url: Option<Url>,
    pub remote_file_info: Option<FileInfo>,
    pub filename: Option<String>,
}

impl Head {
    pub(crate) fn create_inner(
        status: StatusCode,
        headers: &HeaderMap,
        url: &Url,
        updated_url: Option<Url>,
    ) -> Result<Self, Error> {
        if !status.is_success() {
            return Err(Error::UnexpectedStatus {
                status: status.as_u16(),
            });
        }

        let mut result = Self {
            updated_url,
            remote_file_info: None,
            filename: None,
        };

        result.remote_file_info = Some(FileInfo::from_response(status, headers, 0));

        // Get the filename from the server.
        result.filename = crate::headers::parse_content_disposition(headers)
            .map(Cow::<str>::into_owned)
            .or_else(|| {
                let url_filename = url.path().split('/').next_back().unwrap();
                if url_filename.is_empty() {
                    None
                } else {
                    Some(url_filename.to_owned())
                }
            });

        Ok(result)
    }

    /// Return the remote filename.
    pub fn get_remote_file_name(&self) -> &str {
        self.filename.as_deref().unwrap_or("file")
    }

    /// Try to get the length of the remote file.  This may return None if the
    /// server doesn't provide a Content-Length header.
    pub fn get_remote_file_length(&self) -> Option<u64> {
        self.remote_file_info
            .as_ref()
            .and_then(|info| info.file_length)
    }
}

// TODO: unit tests
