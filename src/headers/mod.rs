//! Defines parsers for the various HTTP headers involved in downloading a file.

use http::{HeaderMap, HeaderName, HeaderValue};

mod content_disposition;
mod content_range;

pub use content_disposition::*;
pub use content_range::*;

use crate::file_info::FileInfo;

/// A "weakness indicator" as specified by [RFC-9110 S13.1.5](https://www.rfc-editor.org/info/rfc9110/#section-13.1.5).
const WEAK_ETAG_INDICATOR: &str = "W/";

/// Retrieves a header value as a string slice.
fn get_header_str<'a>(headers: &'a HeaderMap, name: &HeaderName) -> Option<&'a str> {
    headers.get(name).and_then(|value| value.to_str().ok())
}

/// Parses the `Content-Length` header value into a `u64`.
pub fn parse_content_length(headers: &HeaderMap) -> Option<u64> {
    get_header_str(headers, &http::header::CONTENT_LENGTH).and_then(|value| value.parse().ok())
}

pub fn etag(headers: &HeaderMap) -> Option<&str> {
    get_header_str(headers, &http::header::ETAG)
}

/// Returns the value of the "last-modified" header, if present.
pub fn get_last_modified(headers: &HeaderMap) -> Option<&str> {
    get_header_str(headers, &http::header::LAST_MODIFIED)
}

/// Work out the range headers to use to resume the download.
pub fn add_resume_download_headers(
    headers: &mut HeaderMap,
    local_file_info: &FileInfo,
    local_size: u64,
) {
    if local_size > 0 {
        let last_modified = local_file_info.last_modified.as_deref();
        let etag = local_file_info.etag.as_deref();

        // Pick a value to send in the "If-Range" header.  Don't use the etag if
        // it's marked as weak (as per RFC-9110 S13.1.5).
        let if_range = if let Some(etag) = etag
            && !etag.starts_with(WEAK_ETAG_INDICATOR)
        {
            Some(etag)
        } else {
            last_modified
        };

        if let Some(if_range) = if_range {
            headers.insert(
                "Range",
                HeaderValue::from_str(&format!("bytes={local_size}-")).unwrap(),
            );
            headers.insert(
                http::header::IF_RANGE,
                HeaderValue::from_str(if_range).unwrap(),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use http::HeaderValue;

    use super::*;

    fn make_header(k: &'static str, v: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.append(k, HeaderValue::from_str(v).unwrap());
        headers
    }

    #[test]
    fn should_parse_content_length() {
        let length = parse_content_length(&make_header("content-length", "12345"));
        assert_eq!(length, Some(12345));

        let length = parse_content_length(&make_header("content-length", "some nonsense"));
        assert_eq!(length, None);
    }

    #[test]
    fn should_prefer_etag_for_if_range_header() {
        let local_size = 10;
        let local_file_info = FileInfo {
            etag: Some("tag".to_string()),
            last_modified: Some("2023-10-01T12:34:56Z".to_string()),
            file_length: Some(100),
        };

        let mut headers = HeaderMap::new();
        add_resume_download_headers(&mut headers, &local_file_info, local_size);

        let if_range = headers.get(http::header::IF_RANGE).unwrap();
        assert_eq!(if_range, "tag");
    }

    #[test]
    fn should_ignore_weak_etags_for_if_range_header() {
        let local_size = 10;
        let local_file_info = FileInfo {
            etag: Some("W/tag".to_string()),
            last_modified: Some("2023-10-01T12:34:56Z".to_string()),
            file_length: Some(100),
        };

        let mut headers = HeaderMap::new();
        add_resume_download_headers(&mut headers, &local_file_info, local_size);

        let if_range = headers.get(http::header::IF_RANGE).unwrap();
        assert_eq!(if_range, "2023-10-01T12:34:56Z");
    }
}
