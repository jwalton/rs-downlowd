use std::time::Duration;
#[doc = include_str!("../README.md")]
use std::{
    borrow::Cow,
    path::{Path, PathBuf},
    time::SystemTime,
};

mod client;
mod error;
mod file_info;
mod headers;
mod into_url;
mod io_utils;
mod progress;
mod utils;

#[cfg(test)]
mod tests;

use chrono::{DateTime, Utc};
pub use error::Error;
pub use progress::*;
use reqwest::Response;
use tokio::{fs::File, io::AsyncWriteExt};
use url::Url;

use crate::file_info::FileInfo;

pub use client::{Client, ClientBuilder};
pub use http::{HeaderMap, HeaderValue, header::IntoHeaderName};
pub use into_url::IntoUrl;

const DEFAULT_MAX_RETRIES: u64 = 5;

/// Represents a file about to be downloaded.
pub struct Download {
    client: reqwest::Client,
    url: Url,
    updated_url: Option<Url>,
    headers: HeaderMap,
    /// Information we know about the remote file.
    remote_file_info: FileInfo,
    destination: Option<PathBuf>,
    max_retries: u64,
    progress: Option<Box<dyn Progress>>,
    // TODO: Rate limiting
    err: Option<Error>,
}

struct DownloadInner {
    client: reqwest::Client,
    /// The original URL we were trying to download from.
    url: Url,
    /// The URL we are downloading from.  This may change if we follow redirects.
    updated_url: Option<Url>,
    headers: HeaderMap,
    /// Information we know about the remote file.
    remote_file_info: FileInfo,
    /// The maximum number of times we can consecutively retry without making any progress.
    max_retries: u64,
    /// Progress callback, if any.
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
    /// The total number of bytes transfered.
    bytes_transferred: u64,
}

#[derive(Debug, Clone)]
pub struct DownloadResult {
    pub path: PathBuf,
    pub bytes_downloaded: u64,
}

impl Download {
    /// Create a new download for the given URL.
    fn create(client: reqwest::Client, url: impl IntoUrl) -> Self {
        let (url, err) = match url.into_url() {
            Ok(u) => (u, None),
            Err(e) => (
                Url::parse("http://invalid/").unwrap(),
                Some(Error::InvalidUrl {
                    cause: e.to_string(),
                }),
            ),
        };

        Download {
            client,
            url,
            updated_url: None,
            remote_file_info: FileInfo::default(),
            destination: None,
            max_retries: DEFAULT_MAX_RETRIES,
            progress: None,
            headers: HeaderMap::new(),
            err,
        }
    }

    /// Add a custom header for this download.
    pub fn header<K, V>(mut self, key: K, value: V) -> Self
    where
        K: IntoHeaderName,
        HeaderValue: TryFrom<V>,
        <HeaderValue as TryFrom<V>>::Error: Into<http::Error>,
    {
        match value.try_into() {
            Ok(v) => {
                self.headers.insert(key, v);
            }
            Err(e) => {
                self.err = Some(Error::InvalidHeader {
                    cause: e.into().to_string(),
                });
            }
        };

        self
    }

    /// Add multiple headers for this download.
    pub fn add_headers(mut self, headers: HeaderMap) -> Self {
        for (key, value) in headers.iter() {
            self.headers.insert(key, value.clone());
        }
        self
    }

    /// Set the progress reporter for this download.  The given reporter will
    /// be called periodically as data is downloaded.
    pub fn progress(mut self, progress: impl Progress + 'static) -> Self {
        self.progress = Some(Box::new(progress));
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

    /// Set the maxmimum number of times we will consecutively retry without making
    /// any progress. The default is 5. This counter resets if we download at
    /// least one byte of data from the server.
    pub fn max_retries(mut self, max_retries: u64) -> Self {
        self.max_retries = max_retries;
        self
    }

    /// This causes the file to actually be downloaded.
    pub async fn download(mut self) -> Result<DownloadResult, Error> {
        if let Some(e) = self.err {
            return Err(e);
        }

        // Work out where we're ultimately going to save the file.
        let destination = self.resolve_destination().await?;

        // Check to see if the `destination` already exists and, if so, if
        // it's the correct length.
        // TODO: Allow forcing the download if the file already exists.
        let destination_metadata = tokio::fs::metadata(&destination).await.ok();
        if let Some(metadata) = destination_metadata {
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
            updated_url: self.updated_url,
            headers: self.headers,
            remote_file_info,
            max_retries: self.max_retries,
            progress: self.progress,
            destination,
            part_filename,
            sidecar_filename,
            part_file,
            local_file_size,
            bytes_transferred: 0,
        };

        inner.download().await
    }

    /// Returns the final destination path for the download.
    async fn resolve_destination(&mut self) -> Result<PathBuf, Error> {
        let mut destination = match &self.destination {
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

        let is_dir = tokio::fs::metadata(destination.as_ref())
            .await
            .map(|m| m.is_dir())
            .unwrap_or(false);

        // If the destination is a directory, figure out the filename for the file.
        if is_dir {
            let (u, head) = request(
                &self.client,
                reqwest::Method::HEAD,
                self.url.clone(),
                self.headers.clone(),
            )
            .await;
            if u.is_some() {
                self.updated_url = u;
            }

            let url = self.updated_url.as_ref().unwrap_or(&self.url);

            // Need to get the filename from the server.
            let filename = head
                .ok()
                .and_then(|head| {
                    if head.status().is_success() {
                        headers::parse_content_disposition(&head)
                            .map(|s| Cow::Owned(s.into_owned()))
                    } else {
                        url.path().split('/').next_back().map(Cow::Borrowed)
                    }
                })
                .unwrap_or(Cow::Borrowed("file"));
            destination = Cow::Owned(destination.as_ref().join(filename.as_ref()));
        };

        Ok(destination.into_owned())
    }
}

impl DownloadInner {
    async fn download(mut self) -> Result<DownloadResult, Error> {
        let mut tries = 0;
        let mut retries = 0;

        let mut done = false;
        while !done {
            tries += 1;
            retries += 1;

            let bytes_before = self.bytes_transferred;
            match self.try_download(tries).await {
                Ok(()) => {
                    done = true;
                }
                Err(e) => {
                    if matches!(e, Error::FileChanged { .. }) {
                        self.truncate().await?;
                        self.update_remote_file_info(None, None, None).await;
                    } else if !e.can_retry() {
                        // TODO: Make the wait time configurable.  Use some kind of backoff.
                        tokio::time::sleep(Duration::from_secs(1)).await;
                        return Err(e);
                    } else {
                        if self.bytes_transferred > bytes_before {
                            // We made some progress - reset the retry counter.
                            retries = 0;
                        }
                        if retries > self.max_retries {
                            return Err(e);
                        }
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
            bytes_downloaded: self.bytes_transferred,
        })
    }

    /// This is the "inner loop" of the download. Try to download the file, and return
    /// an error if it fails for any reason.  The caller can then decide whether to retry or not.
    async fn try_download(&mut self, tries: u64) -> Result<(), Error> {
        // Make our GET request.
        let response = self.get_file().await?;

        // If the server returns a "206 - Partial content", we're resuming the download,
        // so we should append to the existing file.  Otherwise, we should overwrite it.
        let append = response.status().as_u16() == 206; // Partial content
        let last_modified = headers::parse_last_modified(&response);
        let etag = headers::etag(&response).map(|s| s.to_string());
        let content_length = headers::parse_content_length(&response);

        if append {
            // If we're trying to append to an existing file, but the file has changed on
            // the server, then error.  This SHOULD never happen, thanks to the `If-Range`
            // header we sent, but some servers are not well behaved.
            self.validate_file_unchanged(last_modified, &etag, content_length)?;
        } else {
            // If we're not appending, then update our info about the remote file.
            self.update_remote_file_info(content_length, last_modified, etag)
                .await;
        }

        // Copy data from the response to the .part file.
        self.copy_response_to_file(tries, response, append).await?;

        Ok(())
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

    /// Update the remote file info, and write out the sidecar file if anything has changed.
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

    /// Work out the range headers to use to resume the download.
    fn add_resume_download_headers(&self, headers: &mut HeaderMap) {
        let local_file_size = self.local_file_size;
        let last_modified = self.remote_file_info.modified.as_ref();
        let etag = self.remote_file_info.etag.as_deref();

        if local_file_size > 0 {
            if let Some(if_range) = etag
                .map(Cow::Borrowed)
                .or_else(|| last_modified.map(|dt| Cow::Owned(dt.to_rfc2822())))
            {
                headers.insert(
                    "Range",
                    HeaderValue::from_str(&format!("bytes={local_file_size}-")).unwrap(),
                );
                headers.insert(
                    "If-Range",
                    HeaderValue::from_str(if_range.as_ref()).unwrap(),
                );
            }
        }
    }

    /// Send a GET request for the file.
    async fn get_file(&mut self) -> Result<Response, Error> {
        let mut headers = self.headers.clone();
        self.add_resume_download_headers(&mut headers);
        let url = self.updated_url.as_ref().unwrap_or(&self.url);
        let (u, response) = request(&self.client, reqwest::Method::GET, url.clone(), headers).await;
        if u.is_some() {
            self.updated_url = u;
        }
        response
    }

    async fn truncate(&mut self) -> Result<(), Error> {
        utils::file::truncate_file_async(&self.part_filename, &mut self.part_file).await?;
        self.local_file_size = 0;
        Ok(())
    }

    /// Stream data from the response to a file, and call into the progress callback as we go.
    /// Returns the total number of bytes written to the file, whether or not this succeeds.
    async fn copy_response_to_file(
        &mut self,
        tries: u64,
        mut response: Response,
        append: bool,
    ) -> Result<u64, Error> {
        let mut bytes_downloaded = 0;
        if !append {
            self.truncate().await?;
        }
        let initial_size = self.local_file_size;

        let mut progress_data = ProgressData {
            original_url: &self.url,
            url: self.updated_url.as_ref().unwrap_or(&self.url),
            destination: &self.destination,
            tries,
            bytes_transferred: self.bytes_transferred,
            bytes: initial_size,
            total_bytes: response
                .content_length()
                .map(|v| v + initial_size)
                .or(self.remote_file_info.length),
        };

        // Initial call into the progress callback.
        if let Some(progress) = self.progress.as_mut() {
            progress.progress(&progress_data);
        }

        loop {
            let chunk_result = response.chunk().await.map_err(|cause| Error::Network {
                during: "read",
                url: self.url.to_string(),
                cause,
            })?;
            let chunk = match chunk_result {
                Some(c) => c,
                None => break, // EOF
            };

            self.part_file
                .write_all(&chunk)
                .await
                .map_err(|err| Error::Write {
                    action: "writing to file",
                    path: self.part_filename.to_owned(),
                    cause: err,
                })?;

            let chunk_size = chunk.len() as u64;
            bytes_downloaded += chunk_size;
            self.local_file_size += chunk_size;
            self.bytes_transferred += chunk_size;

            if let Some(progress) = &mut self.progress {
                progress_data.bytes_transferred = self.bytes_transferred;
                progress_data.bytes = initial_size + bytes_downloaded;
                progress.progress(&progress_data);
            }
        }

        Ok(bytes_downloaded)
    }
}

/// Send a request to the server, following redirects as necessary.  Returns the
/// URL we actually fetched from, and the response (or an error).
async fn request(
    client: &reqwest::Client,
    method: reqwest::Method,
    url: Url,
    headers: HeaderMap,
) -> (Option<Url>, Result<reqwest::Response, Error>) {
    let method_name = match method {
        reqwest::Method::GET => "GET",
        reqwest::Method::HEAD => "HEAD",
        _ => "REQUEST",
    };

    // Reqwest follows redirect automatically.
    let response = client
        .request(method, url.clone())
        .headers(headers)
        .send()
        .await
        .map_err(|cause| {
            if cause.is_redirect() {
                Error::BadRedirect {
                    reason: "too many redirects",
                }
            } else {
                Error::Network {
                    during: method_name,
                    url: url.to_string(),
                    cause,
                }
            }
        });

    let response = match response {
        Ok(r) => r,
        Err(e) => {
            println!("Error during request: {e}");
            return (None, Err(e));
        }
    };

    let returned_url = if response.url() != &url {
        Some(response.url().clone())
    } else {
        None
    };

    if !response.status().is_success() {
        return (
            returned_url,
            Err(Error::UnexpectedStatus(response.status().as_u16())),
        );
    }

    (returned_url, Ok(response))
}
