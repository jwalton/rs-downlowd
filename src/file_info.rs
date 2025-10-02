use std::{fs, path::Path};

use crate::{Error, headers};

const FILE_LENGTH_TAG: &str = "File-Length";
const LAST_MODIFIED_TAG: &str = "Last-Modified";
const ETAG_TAG: &str = "Etag";

/// Information we know about a file.
#[derive(Debug, Default)]
pub struct FileInfo {
    pub file_length: Option<u64>,
    pub last_modified: Option<String>,
    pub etag: Option<String>,
}

impl FileInfo {
    /// Create a FileInfo from a reqwest response.  local_file_size can be 0 if
    /// we are not resuming a download.
    pub fn from_reqwest_response(response: &reqwest::Response, local_file_size: u64) -> Self {
        let local_file_size = if response.status().as_u16() == 206 { local_file_size } else { 0 };

        FileInfo {
            // TODO: If the content-length header is missing, use the content-range header instead.
            file_length: headers::parse_content_length(response).map(|len| len + local_file_size),
            last_modified: headers::get_last_modified(response).map(str::to_string),
            etag: headers::etag(response).map(str::to_string),
        }
    }

    /// Serialize the FileInfo to a string that can be stored alongside the part file.
    pub fn serialize(&self) -> String {
        let mut result = String::new();

        if let Some(len) = self.file_length {
            result.push_str(&format!("{FILE_LENGTH_TAG}: {len}\n"));
        }
        if let Some(last_modified) = &self.last_modified {
            result.push_str(&format!("{LAST_MODIFIED_TAG}: {last_modified}\n"));
        }
        if let Some(etag) = &self.etag {
            result.push_str(&format!("{ETAG_TAG}: {etag}\n",));
        }

        result
    }

    /// Deserialize a FileInfo from a string.
    pub fn deserialize(&mut self, s: &str) -> Result<(), Error> {
        self.file_length = None;
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
                FILE_LENGTH_TAG => {
                    if let Ok(v) = value.parse::<u64>() {
                        self.file_length = Some(v);
                    }
                }
                LAST_MODIFIED_TAG => {
                    self.last_modified = Some(value.to_string());
                }
                ETAG_TAG => {
                    self.etag = Some(value.to_string());
                }
                _ => {
                    // Unknown key, ignore it.
                }
            }
        }

        Ok(())
    }

    /// Load the FileInfo from a sidecar file, if it exists.
    pub async fn load(&mut self, sidecar_file: &Path) -> Result<(), Error> {
        let contents = {
            let sidecar_file = sidecar_file.to_owned();
            tokio::task::spawn_blocking(move || fs::read_to_string(sidecar_file))
                .await
                .unwrap()
        };

        match contents {
            Ok(contents) => self.deserialize(&contents),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(Error::Write {
                action: "reading sidecar file",
                cause: e,
                path: sidecar_file.to_path_buf(),
            }),
        }
    }

    /// Write the FileInfo to a sidecar file.
    pub async fn save(&self, sidecar_file: &Path) {
        let serialized = self.serialize();
        let sidecar_file = sidecar_file.to_owned();
        let _ = tokio::task::spawn_blocking(move || fs::write(sidecar_file, serialized)).await;
    }

    pub async fn reset(&mut self, sidecar_file: &Path) {
        self.file_length = None;
        self.last_modified = None;
        self.etag = None;

        let sidecar_file = sidecar_file.to_owned();
        let _ = tokio::task::spawn_blocking(move || fs::remove_file(sidecar_file)).await;
    }

    /// When called on local file info, returns an error if the passed in remote file
    /// info indicates that the file has changed since we last downloaded it.
    ///
    ///
    pub fn verify_unchanged(&self, remote_file_info: &FileInfo) -> Result<(), Error> {
        let mut etag_verified = false;

        match (self.etag.as_deref(), remote_file_info.etag.as_deref()) {
            (Some(local_etag), Some(remote_etag)) => {
                if local_etag != remote_etag {
                    return Err(Error::FileChanged {
                        description: "etag changed",
                    });
                }
                etag_verified = true;
            }
            (Some(_), None) => {
                return Err(Error::FileChanged {
                    description: "server stopped returning etag",
                });
            }
            (None, Some(_)) => {
                // We don't know what the etag was, so we can't verify it.
            }
            (None, None) => {
                // Neither the local file nor the remote file has an etag.
            }
        }

        match (
            self.last_modified.as_deref(),
            remote_file_info.last_modified.as_deref(),
        ) {
            (Some(local_last_modified), Some(remote_last_modified)) => {
                if local_last_modified != remote_last_modified {
                    // TODO: If we already verified the etag, should we skip this error?
                    return Err(Error::FileChanged {
                        description: "last-modified changed",
                    });
                }
            }
            (Some(_), None) => {
                // If we have an etag and it matches, we can ignore the missing last-modified.
                if !etag_verified {
                    return Err(Error::FileChanged {
                        description: "server stopped returning last-modified",
                    });
                }
            }
            (None, Some(_)) => {
                // We don't know what the last modified time was, so we can't verify it.
            }
            (None, None) => {
                // Neither the local file nor the remote file has a last modified time.
            }
        }

        if let (Some(local_length), Some(remote_length)) =
            (self.file_length, remote_file_info.file_length)
        {
            if local_length != remote_length {
                return Err(Error::FileChanged {
                    description: "file size changed",
                });
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"File-Length: 1234
Last-Modified: 2023-10-01T12:34:56Z
Etag: abc123
"#;

    #[test]
    fn test_serialize_serialize() {
        let info = FileInfo {
            file_length: Some(1234),
            last_modified: Some("2023-10-01T12:34:56Z".to_string()),
            etag: Some("abc123".to_string()),
        };

        let serialized = info.serialize();
        assert_eq!(&serialized, SAMPLE);

        let mut deserialized = FileInfo::default();
        deserialized.deserialize(&serialized).unwrap();

        assert_eq!(info.file_length, deserialized.file_length);
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
        assert_eq!(info.file_length, None);
        assert_eq!(info.last_modified, None);
        assert_eq!(info.etag, None);
    }
}
