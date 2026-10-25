use http::{HeaderMap, Method, StatusCode, Uri};

use crate::{
    Error,
    file_info::FileInfo,
    maybe_async::{Client, Response},
    utils::file,
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
        result.filename = Self::remote_filename(headers, uri);

        Ok(result)
    }

    /// Work out the name of the remote file.
    fn remote_filename(headers: &HeaderMap, uri: &Uri) -> Option<String> {
        // Get the filename from the server.
        crate::headers::parse_content_disposition(headers).or_else(|| {
            // If that fails, get the last part of the path.
            let url_filename = uri.path().split('/').next_back().unwrap();
            urlencoding::decode(url_filename)
                .ok()
                .and_then(|f| file::sanitize_filename(&f))
                .filter(|f| !f.is_empty())
        })
    }

    /// Return the remote filename.
    pub fn remote_file_name(&self) -> Option<&str> {
        self.filename.as_deref()
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

#[cfg(test)]
mod tests {
    use super::*;

    use http::{HeaderMap, HeaderValue};

    fn make_headers(k: &'static str, v: Option<&str>) -> HeaderMap {
        let mut headers = HeaderMap::new();
        if let Some(v) = v {
            headers.append(k, HeaderValue::from_str(v).unwrap());
        }
        headers
    }

    #[test]
    pub fn should_sanitize_filename() {
        let check = |uri: &'static str,
                     header_filename: Option<&str>,
                     expected_filename: Option<&str>| {
            let uri = Uri::from_static(uri);
            let header_value = header_filename.map(|f| format!(r#"attachment; filename="{f}""#));
            let headers = make_headers("content-disposition", header_value.as_deref());
            let filename = Head::remote_filename(&headers, &uri);
            assert_eq!(filename.as_deref(), expected_filename);
        };

        // Should read filename from URL if there's no content-disposition header.
        check("http://example.com/file.txt", None, Some("file.txt"));

        // Should prefer the content-disposition header, if it's there.
        check(
            "http://example.com/file.txt",
            Some("preferred.txt"),
            Some("preferred.txt"),
        );

        // Should sanitize the filename.
        check("http://example.com/平和.txt", None, Some("平和.txt"));
        check("http://example.com/.hidden", None, Some("hidden"));
        check("http://example.com/..%2Fevil", None, Some("evil"));
        check("http://example.com/c:evil", None, Some("evil"));
        check("http://example.com/COM1", None, None);
        check("http://example.com/%ff", None, None);
        check("http://example.com/", None, None);
    }
}
