use std::borrow::Cow;

use http::HeaderMap;
use reqwest::Client;
use url::Url;

use crate::{Error, file_info::FileInfo};

/// Information about a URL fetched with a HEAD request.
pub struct Head {
    pub updated_url: Option<Url>,
    pub remote_file_info: Option<FileInfo>,
    pub filename: Option<String>,
}

impl Head {
    /// Send a HEAD request to the server to get information about a URL.
    pub async fn create(client: &Client, url: &Url, headers: HeaderMap) -> Result<Self, Error> {
        // TODO: Retry the HEAD request if it fails with a retryable error.
        let (updated_url, head) =
            crate::utils::reqwest::request(client, reqwest::Method::HEAD, url.clone(), headers)
                .await;

        let mut result = Self {
            updated_url,
            remote_file_info: None,
            filename: None,
        };

        if let Ok(response) = head.as_ref() {
            if !response.status().is_success() {
                return Err(Error::UnexpectedStatus {
                    status: response.status().as_u16(),
                });
            }

            result.remote_file_info = Some(FileInfo::from_reqwest_response(response, 0));

            // Get the filename from the server.
            result.filename = crate::headers::parse_content_disposition(response)
                .map(Cow::<str>::into_owned)
                .or_else(|| {
                    let url_filename = url.path().split('/').next_back().unwrap();
                    if url_filename.is_empty() {
                        None
                    } else {
                        Some(url_filename.to_owned())
                    }
                });
        }

        Ok(result)
    }
}

// TODO: unit tests
