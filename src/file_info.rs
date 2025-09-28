use std::path::Path;

use chrono::{DateTime, Utc};
use tokio::fs;

use crate::Error;

/// Information we know about a file.
#[derive(Debug, Default)]
pub struct FileInfo {
    pub length: Option<u64>,
    pub modified: Option<DateTime<Utc>>,
    pub etag: Option<String>,
}

impl FileInfo {
    /// Serialize the FileInfo to a string that can be stored alongside the part file.
    pub fn serialize(&self) -> String {
        let mut result = String::new();

        if let Some(len) = self.length {
            result.push_str(&format!("Content-Length: {len}\n"));
        }
        if let Some(modified) = &self.modified {
            result.push_str(&format!("Last-Modified: {}\n", modified.to_rfc3339()));
        }
        if let Some(etag) = &self.etag {
            result.push_str(&format!("Etag: {etag}\n",));
        }

        result
    }

    /// Deserialize a FileInfo from a string.
    pub fn deserialize(&mut self, s: &str) -> Result<(), Error> {
        self.length = None;
        self.modified = None;
        self.etag = None;

        for line in s.lines() {
            let mut parts = line.splitn(2, ": ");
            let key = parts.next().unwrap();
            let value = match parts.next() {
                Some(v) => v,
                None => {
                    // Ignore the invalid line.
                    continue;
                }
            };

            match key {
                "Content-Length" => {
                    if let Ok(v) = value.parse::<u64>() {
                        self.length = Some(v);
                    }
                }
                "Last-Modified" => {
                    if let Ok(v) = DateTime::parse_from_rfc3339(value) {
                        self.modified = Some(v.with_timezone(&Utc));
                    }
                }
                "Etag" => {
                    self.etag = Some(value.to_string());
                }
                _ => {
                    // Unknown key, ignore it.
                }
            }
        }

        Ok(())
    }

    /// Update the file info if anything has changed, and persist to disk.
    pub async fn update(
        &mut self,
        sidecar_file: &Path,
        content_length: Option<u64>,
        last_modified: Option<DateTime<Utc>>,
        etag: Option<String>,
    ) {
        let changed =
            self.length != content_length || self.modified != last_modified || self.etag != etag;

        if changed {
            self.length = content_length;
            self.modified = last_modified;
            self.etag = etag;

            if self.length.is_some() || self.modified.is_some() || self.etag.is_some() {
                let serialized = self.serialize();
                let _ = fs::write(sidecar_file, serialized).await;
            } else {
                let _ = fs::remove_file(sidecar_file).await;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"Content-Length: 1234
Last-Modified: 2023-10-01T12:34:56+00:00
Etag: abc123
"#;

    #[test]
    fn test_serialize_serialize() {
        let info = FileInfo {
            length: Some(1234),
            modified: Some(
                DateTime::parse_from_rfc3339("2023-10-01T12:34:56Z")
                    .unwrap()
                    .with_timezone(&Utc),
            ),
            etag: Some("abc123".to_string()),
        };

        let serialized = info.serialize();
        assert_eq!(&serialized, SAMPLE);

        let mut deserialized = FileInfo::default();
        deserialized.deserialize(&serialized).unwrap();

        assert_eq!(info.length, deserialized.length);
        assert_eq!(info.modified, deserialized.modified);
        assert_eq!(info.etag, deserialized.etag);
    }

    #[test]
    fn should_deserialize_etags_with_special_characters() {
        let mut info = FileInfo::default();
        info.deserialize(r#"Etag: q"abc/123:=+xyz"#).unwrap();
        assert_eq!(info.etag, Some(r#"q"abc/123:=+xyz"#.to_string()));
    }

    #[test]
    fn should_deserialize_empty_file() {
        let mut info = FileInfo::default();
        info.deserialize("").unwrap();
        assert_eq!(info.length, None);
        assert_eq!(info.modified, None);
        assert_eq!(info.etag, None);
    }
}
