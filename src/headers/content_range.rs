use http::HeaderMap;

use crate::headers::get_header_str;

#[derive(Debug, PartialEq, Eq)]
pub struct Range {
    pub start: u64,
    pub end: u64,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ContentRange {
    pub range: Option<Range>,
    pub total: Option<u64>,
}

/// Parse the `Content-Range` header.
pub fn parse_content_range(headers: &HeaderMap) -> Option<ContentRange> {
    get_header_str(headers, &http::header::CONTENT_RANGE).and_then(ContentRange::from_str)
}

impl ContentRange {
    pub fn from_str(value: &str) -> Option<Self> {
        let value = value.strip_prefix("bytes")?;
        let slash = value.find("/")?;
        let (range_part, total_part) = value.split_at(slash);

        let range = Range::from_str(range_part);

        let total = if total_part.trim() == "/*" {
            None
        } else {
            total_part.trim_start_matches("/").trim().parse().ok()
        };

        Some(Self { range, total })
    }
}

impl Range {
    pub fn from_str(value: &str) -> Option<Self> {
        if value.trim() == "*" {
            return None;
        }

        let dash = value.find("-")?;
        let (start, end) = value.split_at(dash);
        let (start, end) = (start.trim(), end.trim_start_matches("-").trim());

        Some(Self {
            start: start.parse().ok()?,
            end: end.parse().ok()?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_parse_content_range() {
        let content_range = ContentRange::from_str("bytes 0-499/1234");
        assert_eq!(
            content_range,
            Some(ContentRange {
                range: Some(Range { start: 0, end: 499 }),
                total: Some(1234)
            })
        );

        let content_range = ContentRange::from_str("bytes 0-499/*");
        assert_eq!(
            content_range,
            Some(ContentRange {
                range: Some(Range { start: 0, end: 499 }),
                total: None
            })
        );

        let content_range = ContentRange::from_str("bytes */1234");
        assert_eq!(
            content_range,
            Some(ContentRange {
                range: None,
                total: Some(1234)
            })
        );

        let content_range = ContentRange::from_str("fish 10-20/20");
        assert_eq!(content_range, None);

        let content_range = ContentRange::from_str("");
        assert_eq!(content_range, None);

    }
}
