use std::borrow::Cow;

use http::{HeaderMap, Method, StatusCode, Uri};

use crate::{
    Error,
    file_info::FileInfo,
    maybe_async::{Client, Response},
};

/// Information about a URL fetched with a HEAD request.
#[derive(Debug, Default)]
pub struct Head {
    pub updated_uri: Option<Uri>,
    pub remote_file_info: Option<FileInfo>,
    pub filename: Option<String>,
}

/// Lazily-initialized information about a URL fetched with a HEAD request.
#[derive(Default)]
pub struct LazyHead {
    head: Option<Head>,
}

impl Head {
    pub async fn create<C: Client>(client: &C, uri: &Uri, headers: &HeaderMap) -> Head {
        // TODO: Retry the HEAD request if it fails with a retryable error.
        let (updated_uri, head) = client.request(Method::HEAD, uri, headers.clone()).await;

        if let Ok(response) = head {
            Head::create_inner(response.status(), response.headers(), uri, updated_uri)
                .unwrap_or_default()
        } else {
            Head::default()
        }
    }

    fn create_inner(
        status: StatusCode,
        headers: &HeaderMap,
        uri: &Uri,
        updated_uri: Option<Uri>,
    ) -> Result<Self, Error> {
        if !status.is_success() {
            return Err(Error::UnexpectedStatus {
                status: status.as_u16(),
            });
        }

        let mut result = Self {
            updated_uri,
            remote_file_info: None,
            filename: None,
        };

        result.remote_file_info = Some(FileInfo::from_response(status, headers, 0));

        // Get the filename from the server.
        result.filename = crate::headers::parse_content_disposition(headers)
            .map(Cow::<str>::into_owned)
            .or_else(|| {
                let url_filename = uri.path().split('/').next_back().unwrap();
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

impl LazyHead {
    /// Get the Head object for the given URL.
    pub async fn get<C: Client>(&mut self, client: &C, uri: &Uri, headers: &HeaderMap) -> &Head {
        if self.head.is_none() {
            let h = Head::create(client, uri, headers).await;
            self.head.replace(h);
        }

        self.head.as_ref().unwrap()
    }

    pub fn try_get(&self) -> Option<&Head> {
        self.head.as_ref()
    }
}

// TODO: unit tests
