use std::borrow::Cow;

use http::{HeaderMap, HeaderName, HeaderValue};

mod content_range;

pub use content_range::*;

use crate::ProgressHandle;

/// Retrieves a header value as a string slice.
fn get_header_str<'a>(headers: &'a HeaderMap, name: &HeaderName) -> Option<&'a str> {
    headers.get(name).and_then(|value| value.to_str().ok())
}

/// Parses the `Content-Length` header value into a `u64`.
pub fn parse_content_length(headers: &HeaderMap) -> Option<u64> {
    get_header_str(headers, &http::header::CONTENT_LENGTH).and_then(|value| value.parse().ok())
}

/// Parses the filename from the `Content-Disposition` header.
pub fn parse_content_disposition(headers: &'_ HeaderMap) -> Option<Cow<'_, str>> {
    if let Some(value) = get_header_str(headers, &http::header::CONTENT_DISPOSITION) {
        let mut result: Option<Cow<str>> = None;

        value.split(';').for_each(|part| {
            let trimmed = part.trim();
            result = if trimmed.starts_with("filename=") && result.is_none() {
                Some(Cow::Borrowed(
                    trimmed.trim_start_matches("filename=").trim_matches('"'),
                ))
            } else if trimmed.starts_with("filename*=UTF-8''") {
                urlencoding::decode(trimmed.trim_start_matches("filename*=UTF-8''")).ok()
            } else {
                None
            }
        });

        result
    } else {
        None
    }
}

pub fn etag(headers: &HeaderMap) -> Option<&str> {
    get_header_str(headers, &http::header::ETAG)
}

/// Returns the value of the last_modified header, if present.
pub fn get_last_modified(headers: &HeaderMap) -> Option<&str> {
    get_header_str(headers, &http::header::LAST_MODIFIED)
}

/// Work out the range headers to use to resume the download.
pub fn add_resume_download_headers(headers: &mut HeaderMap, progress: &ProgressHandle) {
    if progress.bytes > 0 {
        let last_modified = progress.local_file_info.last_modified.as_deref();
        let etag = progress.local_file_info.etag.as_deref();

        if let Some(if_range) = etag.or(last_modified) {
            headers.insert(
                "Range",
                HeaderValue::from_str(&format!("bytes={}-", progress.bytes)).unwrap(),
            );
            headers.insert("If-Range", HeaderValue::from_str(if_range).unwrap());
        }
    }
}

#[cfg(test)]
mod tests {
    use http::HeaderValue;

    use super::*;

    fn make_header(k: &'static str, v: &'static str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.append(k, HeaderValue::from_static(v));
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
    fn should_parse_content_disposition() {
        let header = make_header(
            "content-disposition",
            r#"attachment; filename="example.txt""#,
        );
        let filename = parse_content_disposition(&header);
        assert_eq!(filename, Some("example.txt".into()));

        let header = make_header(
            "content-disposition",
            r#"attachment; filename*=UTF-8''file%20name.jpg"#,
        );
        let filename = parse_content_disposition(&header);
        assert_eq!(filename, Some("file name.jpg".into()));

        // If there's a filename and a filename* in the same header,
        // filename* should take precedence.
        let header = make_header(
            "content-disposition",
            r#"attachment; filename="foo.txt"; filename*=UTF-8''file%20name.jpg"#,
        );
        let filename = parse_content_disposition(&header);
        assert_eq!(filename, Some("file name.jpg".into()));
    }
}
