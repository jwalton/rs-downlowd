use std::borrow::Cow;

use http::HeaderName;
use reqwest::Response;

/// Retrieves a header value as a string slice.
fn get_header_str<'a>(response: &'a Response, name: &HeaderName) -> Option<&'a str> {
    response
        .headers()
        .get(name)
        .and_then(|value| value.to_str().ok())
}

/// Parses the `Content-Length` header value into a `u64`.
pub fn parse_content_length(response: &Response) -> Option<u64> {
    get_header_str(response, &reqwest::header::CONTENT_LENGTH).and_then(parse_content_length_str)
}

fn parse_content_length_str(value: &str) -> Option<u64> {
    value.parse().ok()
}

/// Parses the filename from the `Content-Disposition` header.
pub fn parse_content_disposition(response: &Response) -> Option<Cow<str>> {
    get_header_str(response, &reqwest::header::CONTENT_DISPOSITION)
        .and_then(parse_content_disposition_str)
}

fn parse_content_disposition_str(value: &str) -> Option<Cow<str>> {
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
}

pub fn etag(response: &Response) -> Option<&str> {
    get_header_str(response, &reqwest::header::ETAG)
}

/// Returns the value of the last_modified header, if present.
pub fn get_last_modified(response: &Response) -> Option<&str> {
    get_header_str(response, &reqwest::header::LAST_MODIFIED)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_parse_content_length() {
        let length = parse_content_length_str("12345");
        assert_eq!(length, Some(12345));

        let length = parse_content_length_str("some nonsense");
        assert_eq!(length, None);
    }

    #[test]
    fn should_parse_content_disposition() {
        let filename = parse_content_disposition_str(r#"attachment; filename="example.txt""#);
        assert_eq!(filename, Some("example.txt".into()));

        let filename =
            parse_content_disposition_str(r#"attachment; filename*=UTF-8''file%20name.jpg"#);
        assert_eq!(filename, Some("file name.jpg".into()));

        // If there's a filename and a filename* in the same header,
        // filename* should take precedence.
        let filename = parse_content_disposition_str(
            r#"attachment; filename="foo.txt"; filename*=UTF-8''file%20name.jpg"#,
        );
        assert_eq!(filename, Some("file name.jpg".into()));
    }
}
