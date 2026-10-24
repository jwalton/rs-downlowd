use std::borrow::Cow;

use http::HeaderMap;

use crate::{headers::get_header_str, utils::file};

/// Parses the filename from the `Content-Disposition` header.
pub fn parse_content_disposition(headers: &'_ HeaderMap) -> Option<String> {
    if let Some(value) = get_header_str(headers, &http::header::CONTENT_DISPOSITION) {
        let mut result: Option<String> = None;

        value.split(';').for_each(|part| {
            let trimmed = part.trim();
            result = if trimmed.starts_with("filename=") && result.is_none() {
                // Remove "filename=".
                let mut trimmed = &trimmed[9..];

                // Remove leading and trailing '"'s.
                if trimmed.starts_with('"') {
                    trimmed = &trimmed[1..];
                }
                if trimmed.ends_with('"') {
                    trimmed = &trimmed[..trimmed.len() - 1];
                }

                urlencoding::decode(trimmed)
                    .ok()
                    .and_then(|f| file::sanitize_filename(&f))
            } else if trimmed.starts_with("filename*=UTF-8''") {
                urlencoding::decode(trimmed.trim_start_matches("filename*=UTF-8''"))
                    .ok()
                    .map(Cow::into_owned)
            } else {
                None
            }
        });

        result
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use http::HeaderValue;

    use super::*;

    fn make_headers(k: &'static str, v: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.append(k, HeaderValue::from_str(v).unwrap());
        headers
    }

    #[test]
    fn should_parse_content_disposition() {
        let header = make_headers(
            "content-disposition",
            r#"attachment; filename="example.txt""#,
        );
        let filename = parse_content_disposition(&header);
        assert_eq!(filename, Some("example.txt".into()));

        let header = make_headers(
            "content-disposition",
            r"attachment; filename*=UTF-8''file%20name.jpg",
        );
        let filename = parse_content_disposition(&header);
        assert_eq!(filename, Some("file name.jpg".into()));

        // If there's a filename and a filename* in the same header,
        // filename* should take precedence.
        let header = make_headers(
            "content-disposition",
            r#"attachment; filename="foo.txt"; filename*=UTF-8''file%20name.jpg"#,
        );
        let filename = parse_content_disposition(&header);
        assert_eq!(filename, Some("file name.jpg".into()));

        // Should ignore an empty header.
        let header = make_headers("content-disposition", r#"attachment; filename="""#);
        let filename = parse_content_disposition(&header);
        assert_eq!(filename, None);
    }

    #[test]
    fn should_handle_content_disposition_edge_cases() {
        let check = |header_filename: &str, expected_filename: Option<&str>| {
            let header_value = format!(r#"attachment; filename="{header_filename}""#);
            let headers = make_headers("content-disposition", &header_value);
            let filename = parse_content_disposition(&headers);
            assert_eq!(filename.as_deref(), expected_filename);
        };

        // Should sanitize the content-disposition header if it's something weird.
        check("/etc/passwd", Some("passwd"));
        check("../evil", Some("evil"));
        check("..%2Fevil", Some("evil"));
        check("COM1", None);
        check("c:evil.txt", Some("evil.txt"));
        check(".hidden", Some("hidden"));

        // The `http::HeaderMap::get` function will return None for a header with
        // non-ascii characters in it, so these will end up "None".
        check("emo😊ji", None);
        check("平和", None);

        // But these should work.
        check(&urlencoding::encode("emo😊ji"), Some("emo😊ji"));
        check(&urlencoding::encode("平和"), Some("平和"));

    }
}
