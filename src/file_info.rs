use std::{fs, path::Path};

use crate::Error;

/// Information we know about a file.
#[derive(Debug, Default)]
pub struct FileInfo {
    pub content_length: Option<u64>,
    pub last_modified: Option<String>,
    pub etag: Option<String>,
}

impl FileInfo {
    /// Serialize the FileInfo to a string that can be stored alongside the part file.
    pub fn serialize(&self) -> String {
        let mut result = String::new();

        if let Some(len) = self.content_length {
            result.push_str(&format!("Content-Length: {len}\n"));
        }
        if let Some(last_modified) = &self.last_modified {
            result.push_str(&format!("Last-Modified: {last_modified}\n"));
        }
        if let Some(etag) = &self.etag {
            result.push_str(&format!("Etag: {etag}\n",));
        }

        result
    }

    /// Deserialize a FileInfo from a string.
    pub fn deserialize(&mut self, s: &str) -> Result<(), Error> {
        self.content_length = None;
        self.last_modified = None;
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
                        self.content_length = Some(v);
                    }
                }
                "Last-Modified" => {
                    self.last_modified = Some(value.to_string());
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
        last_modified: Option<&str>,
        etag: Option<&str>,
    ) {
        let serialized = self.update_inner(content_length, last_modified, etag);
        let sidecar_file = sidecar_file.to_owned();

        tokio::task::spawn_blocking(move || match serialized {
            Some(serialized) => {
                let _ = fs::write(sidecar_file, serialized);
            }
            None => {
                let _ = fs::remove_file(sidecar_file);
            }
        })
        .await
        .unwrap();
    }

    pub fn update_inner(
        &mut self,
        content_length: Option<u64>,
        last_modified: Option<&str>,
        etag: Option<&str>,
    ) -> Option<String> {
        let mut serialized = None;

        let changed = self.content_length != content_length
            || self.last_modified.as_deref() != last_modified
            || self.etag.as_deref() != etag;

        if changed {
            self.content_length = content_length;
            self.last_modified = last_modified.map(str::to_string);
            self.etag = etag.map(str::to_string);

            if self.content_length.is_some() || self.last_modified.is_some() || self.etag.is_some()
            {
                serialized = Some(self.serialize());
            }
        }

        serialized
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"Content-Length: 1234
Last-Modified: 2023-10-01T12:34:56Z
Etag: abc123
"#;

    #[test]
    fn test_serialize_serialize() {
        let info = FileInfo {
            content_length: Some(1234),
            last_modified: Some("2023-10-01T12:34:56Z".to_string()),
            etag: Some("abc123".to_string()),
        };

        let serialized = info.serialize();
        assert_eq!(&serialized, SAMPLE);

        let mut deserialized = FileInfo::default();
        deserialized.deserialize(&serialized).unwrap();

        assert_eq!(info.content_length, deserialized.content_length);
        assert_eq!(info.last_modified, deserialized.last_modified);
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
        assert_eq!(info.content_length, None);
        assert_eq!(info.last_modified, None);
        assert_eq!(info.etag, None);
    }
}
