use std::{
    borrow::Cow,
    path::{Path, PathBuf},
    time::SystemTime,
};

mod error;
mod file_info;
mod headers;
mod io_utils;
mod progress;
mod utils;

#[cfg(test)]
mod tests;

use chrono::{DateTime, Utc};
pub use error::Error;
pub use progress::Progress;
use reqwest::Response;
use tokio::{fs::File, io::AsyncWriteExt};

use crate::file_info::FileInfo;

/// A client for downloading files over HTTP.
pub struct Client {
    client: reqwest::Client,
    // TODO: Custom headers.
}

/// Represents a file about to be downloaded.
pub struct Download<'a> {
    client: reqwest::Client,
    url: &'a str,
    /// Information we know about the remote file.
    remote_file_info: FileInfo,
    destination: Option<PathBuf>,
    progress: Option<Box<dyn Progress>>,
    // TODO: Custom headers
    // TODO: Rate limiting
    // TODO: Retry logic
}

struct DownloadInner<'a> {
    client: reqwest::Client,
    url: &'a str,
    /// Information we know about the remote file.
    remote_file_info: FileInfo,
    progress: Option<Box<dyn Progress>>,
    /// Final destination for the downloaded file. (e.g. "file.txt")
    destination: PathBuf,
    /// Temporary file we'll write while we're downloading (e.g. "file.txt.part")
    part_filename: PathBuf,
    ///  "Sidecar" file where we'll store info about the file (the etag, the last modified, etc...).  (e.g. "file.txt.downloadinfo")
    sidecar_filename: PathBuf,
    /// File we're writing to.
    part_file: File,
    /// Current size of the local file.
    local_file_size: u64,
}

pub struct DownloadResult {
    pub path: PathBuf,
    pub bytes_downloaded: u64,
}

impl Client {
    // TODO: Allow setting the user_agent after the fact.
    pub fn new(user_agent: String) -> Self {
        let client = reqwest::Client::builder()
            .user_agent(&user_agent)
            .build()
            .expect("Failed to create HTTP client");
        Client { client }
    }

    /// Create a file download.
    ///
    /// example:
    ///
    /// ```rust
    /// let client = Client::new("my-agent/1.0".to_string());
    /// let result = client.download("https://example.com/file.txt")
    ///    .destination("file.txt")
    ///    .download()
    ///    .await?;
    /// ```
    ///
    // TODO: Change this to take an IntoUrl trait?
    pub fn download<'a>(&self, url: &'a str) -> Download<'a> {
        Download::new(self.client.clone(), url)
    }
}

impl<'a> Download<'a> {
    /// Create a new download for the given URL.
    fn new(client: reqwest::Client, url: &'a str) -> Self {
        Download {
            client,
            url,
            remote_file_info: FileInfo::default(),
            destination: None,
            progress: None,
        }
    }

    /// Set the progress reporter for this download.  The given reporter will
    /// be called periodically as data is downloaded.
    pub fn progress(mut self, progress: Box<dyn Progress>) -> Self {
        self.progress = Some(progress);
        self
    }

    /// Set the destination path for the downloaded file.  This can be a file to
    /// store the resulting download in, or a directory in which case the
    /// filename will be determined from the URL or the server's `Content-Disposition`
    /// header.  If this is not set, the current working directory will be used.
    ///
    /// Downloaded files will be saved to a temporary `.part` file in the same
    /// folder, and then renamed to the final destination when the download is complete.
    ///
    pub fn destination(mut self, destination: impl AsRef<Path>) -> Self {
        self.destination = Some(destination.as_ref().to_owned());
        self
    }

    /// Set the etag for this file.  If you have already downloaded part of the
    /// file and know the etag, setting this will allow the download to verify
    /// that the file has not changed on the server before resuming.  If neither
    /// this nor the last modified time are set, then the mtime of the existing
    /// file on disk will be used in place of the last modified time.
    pub fn etag(mut self, etag: impl Into<String>) -> Self {
        self.remote_file_info.etag = Some(etag.into());
        self
    }

    /// Set the last modified time for this file.  If you have already downloaded
    /// part of the file and know the last modified time, setting this will allow
    /// the download to verify that the file has not changed on the server before
    /// resuming.  If neither this nor the etag are set, then the mtime of the existing
    /// file on disk will be used in place of the last modified time.
    pub fn last_modified(mut self, last_modified: SystemTime) -> Self {
        self.remote_file_info.modified = Some(last_modified.into());
        self
    }

    /// This causes the file to actually be downloaded.
    pub async fn download(self) -> Result<DownloadResult, Error> {
        // Work out where we're ultimately going to save the file.
        let destination = self.get_destination().await?;

        // Check to see if the `destination` already exists and, if so, if
        // it's the correct length.
        let destination_metadata = tokio::fs::metadata(&destination).await.ok();
        if let Some(metadata) = destination_metadata {
            // TODO: Do we also want to check the last modified time against the file's mtime?
            // How reliable is that on non-UNIX filesystems?
            if let Some(remote_length) = self.remote_file_info.length {
                if metadata.len() == remote_length {
                    // File already exists and is the correct length - nothing to do.
                    return Ok(DownloadResult {
                        path: destination,
                        bytes_downloaded: 0,
                    });
                }
            }
        }

        // This is a temporary file we'll write while we're downloading.
        let part_filename = utils::file::add_extension(&destination, "part");
        // And this is a "sidecar" file where we'll store info about the file (the etag, the last modified, etc...)
        let sidecar_filename = utils::file::add_extension(&destination, "downloadinfo");

        let mut remote_file_info = self.remote_file_info;
        if remote_file_info.etag.is_some() || remote_file_info.modified.is_some() {
            // User provided some info about the file - use it.
        } else {
            // See if there's a sidecar file with info about the file.
            let contents = tokio::fs::read_to_string(&sidecar_filename).await.ok();
            if let Some(contents) = contents {
                remote_file_info.deserialize(&contents).ok();
            }
        }

        let (part_file, local_file_size) =
            utils::file::open_file_for_writing_async(&part_filename).await?;

        let inner = DownloadInner {
            client: self.client,
            url: self.url,
            remote_file_info,
            progress: self.progress,
            destination,
            part_filename,
            sidecar_filename,
            part_file,
            local_file_size,
        };

        inner.download().await
    }

    /// Returns the final destination path for the download, and the last modified time
    /// and size of the file if the file already exists.
    async fn get_destination(&self) -> Result<PathBuf, Error> {
        // FIXME: Cache this value, so we don't have to recompute it every retry.
        // Also, would be nice if the user could get this value before starting the download,
        // and then we also don't want to have to recompute it.
        let mut destination = self.resolved_destination()?;

        let is_dir = tokio::fs::metadata(destination.as_ref())
            .await
            .map(|m| m.is_dir())
            .unwrap_or(false);

        // If the destination is a directory, figure out the filename for the file.
        if is_dir {
            // Need to get the filename from the server.
            let filename = self
                .client
                .head(self.url)
                .send()
                .await
                .ok()
                .and_then(|head| {
                    if head.status().is_success() {
                        headers::parse_content_disposition(&head)
                            .map(|s| Cow::Owned(s.into_owned()))
                    } else {
                        self.url.split('/').next_back().map(Cow::Borrowed)
                    }
                })
                .unwrap_or(Cow::Borrowed("file"));
            destination = Cow::Owned(destination.as_ref().join(filename.as_ref()));
        };

        Ok(destination.into_owned())
    }

    /// Get the configured destination to store files in.  If self.destination is
    /// None, this will return the resolved current directory.
    fn resolved_destination(&self) -> Result<Cow<Path>, Error> {
        let result = match &self.destination {
            Some(path) => Cow::Borrowed(path.as_path()),
            None => {
                let cwd = std::env::current_dir().map_err(|e| Error::Write {
                    action: "determining current directory",
                    path: PathBuf::from("."),
                    cause: e,
                })?;
                Cow::Owned(cwd)
            }
        };

        Ok(result)
    }
}

impl<'a> DownloadInner<'a> {
    async fn download(mut self) -> Result<DownloadResult, Error> {
        // TODO: Set limits on how many times we retry.
        let mut done = false;
        let mut bytes_downloaded = 0;
        while !done {
            let (n, result) = self.try_download().await;
            self.local_file_size += n;
            bytes_downloaded += n;

            match result {
                Ok(()) => {
                    done = true;
                }
                Err(e) => {
                    if matches!(e, Error::FileChanged { .. }) {
                        bytes_downloaded = 0;
                        self.local_file_size = 0;
                        utils::file::truncate_file_async(&self.part_filename, &mut self.part_file)
                            .await?;
                        self.update_remote_file_info(None, None, None).await;
                    }

                    // Retry on network errors
                    if !e.can_retry() {
                        return Err(e);
                    }
                }
            }
        }

        // We're all done! Close the part_file.
        self.part_file.flush().await.ok();
        drop(self.part_file);

        // Rename the .part file to the final destination, and delete the sidecar file.
        io_utils::finalize_download(
            &self.part_filename,
            &self.sidecar_filename,
            &self.destination,
            self.remote_file_info.modified.as_ref(),
        )
        .await?;

        Ok(DownloadResult {
            path: self.destination,
            bytes_downloaded,
        })
    }

    /// This is the "inner loop" of the download. Try to download the file, and return
    /// an error if it fails for any reason.  The caller can then decide whether to retry or not.
    async fn try_download(&mut self) -> (u64, Result<(), Error>) {
        // Check to see if our part file already exists, and if so, whether we can resume the download.
        let range_headers = resume_download_headers(
            self.local_file_size,
            self.remote_file_info.modified.as_ref(),
            self.remote_file_info.etag.as_deref(),
        );

        // Make our GET request.
        let response = self.get_file(range_headers).await;
        let response = match response {
            Ok(r) => r,
            Err(e) => return (0, Err(e)),
        };

        // If the server returns a "206 - Partial content", we're resuming the download,
        // so we should append to the existing file.  Otherwise, we should overwrite it.
        let append = response.status().as_u16() == 206; // Partial content
        let last_modified = headers::parse_last_modified(&response);
        let etag = headers::etag(&response).map(|s| s.to_string());
        let content_length = headers::parse_content_length(&response);

        // If we're trying to append to an existing file, but the file has changed on
        // the server, then error.  This SHOULD never happen, thanks to the `If-Range`
        // header we sent, but some servers are not well behaved.
        if append {
            if let Err(err) = self.validate_file_unchanged(last_modified, &etag, content_length) {
                return (0, Err(err));
            }
        }

        // Copy data from the response to the .part file.
        let (bytes_written, result) = self.copy_response_to_file(response, append).await;

        // Copy the etag and last modified time from the response.
        if bytes_written > 0 {
            self.update_remote_file_info(content_length, last_modified, etag)
                .await;
        }

        (bytes_written, result)
    }

    fn validate_file_unchanged(
        &mut self,
        last_modified: Option<DateTime<Utc>>,
        etag: &Option<String>,
        content_length: Option<u64>,
    ) -> Result<(), Error> {
        if self.remote_file_info.etag.is_some() && etag.is_some() {
            if self.remote_file_info.etag != *etag {
                return Err(Error::FileChanged {
                    description: "etag changed",
                });
            }
        } else {
            // Only check last-modified if we don't have an etag.  If the modified
            // time has changed, but the etag is the same, assume the file hasn't
            // actually changed.
            if self.remote_file_info.modified.is_some()
                && last_modified.is_some()
                && self.remote_file_info.modified != last_modified
            {
                return Err(Error::FileChanged {
                    description: "last modified time changed",
                });
            }
        }

        // TODO: If the content-length header is missing, use the content-range header instead.
        if let (Some(remote_length), Some(content_length)) =
            (self.remote_file_info.length, content_length)
        {
            let final_length = self.local_file_size + content_length;
            if remote_length != final_length {
                return Err(Error::FileChanged {
                    description: "file size changed",
                });
            }
        }

        Ok(())
    }

    async fn update_remote_file_info(
        &mut self,
        content_length: Option<u64>,
        last_modified: Option<DateTime<Utc>>,
        etag: Option<String>,
    ) {
        let info = &mut self.remote_file_info;

        let changed =
            info.length != content_length || info.modified != last_modified || info.etag != etag;

        if changed {
            info.length = content_length;
            info.modified = last_modified;
            info.etag = etag;

            if info.length.is_some() || info.modified.is_some() || info.etag.is_some() {
                // TODO: Don't write the sidecar file if we were provided the etag or last modified dates
                // by the user?
                // Write out the sidecar file with info about the file.
                let contents = self.remote_file_info.serialize();
                let _ = tokio::fs::write(&self.sidecar_filename, contents).await;
            } else {
                // No info about the file - delete any existing sidecar file.
                let _ = tokio::fs::remove_file(&self.sidecar_filename).await;
            }
        }
    }

    /// Send a GET request for the file.
    async fn get_file<'v>(
        &self,
        headers: Option<Vec<(&str, Cow<'v, str>)>>,
    ) -> Result<Response, Error> {
        // TODO: Allow adding custom headers to the request.
        let mut request = self.client.get(self.url);
        if let Some(headers) = headers {
            for (name, value) in headers {
                request = request.header(name, value.into_owned());
            }
        };

        let response = request.send().await.map_err(|cause| Error::Network {
            during: "GET",
            url: self.url.to_string(),
            cause,
        })?;
        if !response.status().is_success() {
            return Err(Error::UnexpectedStatus(response.status().as_u16()));
        }
        Ok(response)
    }

    /// Stream data from the response to a file, and call into the progress callback as we go.
    /// Returns the total number of bytes written to the file, whether or not this succeeds.
    async fn copy_response_to_file(
        &mut self,
        mut response: Response,
        append: bool,
    ) -> (u64, Result<(), Error>) {
        let mut bytes_downloaded = 0;
        if !append {
            let result =
                utils::file::truncate_file_async(&self.part_filename, &mut self.part_file).await;
            self.local_file_size = 0;
            if result.is_err() {
                return (0, result);
            }
        }
        let initial_size = self.local_file_size;

        // FIXME: Probably want to send `None` to the progress callback if we don't know the total size.
        let total_bytes = response
            .content_length()
            .map(|v| v + initial_size)
            .unwrap_or(0);

        // Initial call into the progress callback.
        if let Some(progress) = self.progress.as_mut() {
            progress.progress(initial_size, total_bytes);
        }

        loop {
            let chunk_result = response.chunk().await.map_err(|cause| Error::Network {
                during: "read",
                url: self.url.to_string(),
                cause,
            });
            let chunk = match chunk_result {
                Ok(Some(c)) => c,
                Ok(None) => break, // EOF
                Err(e) => return (bytes_downloaded, Err(e)),
            };

            bytes_downloaded += chunk.len() as u64;

            if let Err(err) = self.part_file.write_all(&chunk).await {
                return (
                    bytes_downloaded,
                    Err(Error::Write {
                        action: "writing to file",
                        path: self.part_filename.to_owned(),
                        cause: err,
                    }),
                );
            }

            if let Some(progress) = &mut self.progress {
                progress.progress(initial_size + bytes_downloaded, total_bytes);
            }
        }

        (bytes_downloaded, Ok(()))
    }
}

/// Work out the range headers to use to resume the download.
fn resume_download_headers<'a>(
    local_file_size: u64,
    last_modified: Option<&'a DateTime<Utc>>,
    etag: Option<&'a str>,
) -> Option<Vec<(&'static str, Cow<'a, str>)>> {
    if local_file_size > 0 {
        if let Some(if_range) = etag
            .map(Cow::Borrowed)
            .or_else(|| last_modified.map(|dt| Cow::Owned(dt.to_rfc2822())))
        {
            return Some(vec![
                ("Range", Cow::Owned(format!("bytes={local_file_size}-"))),
                ("If-Range", if_range),
            ]);
        }
    }

    // Can't resume the download.
    None
}
