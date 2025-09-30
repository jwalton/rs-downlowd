#[doc = include_str!("../README.md")]
use std::{
    borrow::Cow,
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

mod client;
mod error;
mod file_info;
mod headers;
mod into_url;
mod io_utils;
mod progress;
mod retry;
mod time;
mod utils;

#[cfg(test)]
mod tests;

use chrono::{DateTime, Utc};
pub use error::Error;
use http::Method;
pub use progress::*;
use reqwest::Response;
use tokio::{fs::File, io::AsyncWriteExt};
use url::Url;

use crate::file_info::FileInfo;

pub use client::{Client, ClientBuilder};
pub use http::{HeaderMap, HeaderValue, header::IntoHeaderName};
pub use into_url::IntoUrl;
pub use retry::RetryHandle;
pub use time::exponential_backoff;

pub type RetryHandler = Box<dyn FnMut(&mut RetryHandle)>;

const DEFAULT_MAX_RETRIES: u64 = 5;
const DEFAULT_MIN_DELAY: Duration = Duration::from_millis(500);
const DEFAULT_MAX_DELAY: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    Downloaded,
    Skipped,
}

fn default_retry_callback(handle: &mut RetryHandle) {
    if matches!(handle.error(), Error::FileChanged { .. }) {
        // No delay if the file changed.
        handle.set_delay(Duration::ZERO);
    } else {
        handle.set_delay(exponential_backoff(
            DEFAULT_MIN_DELAY,
            DEFAULT_MAX_DELAY,
            handle.retries(),
        ));
    }
}

/// Represents a file about to be downloaded.
pub struct Download {
    /// The client to use to download the file.
    client: reqwest::Client,
    /// The URL we want to download from.
    url: Url,
    /// If we are redirected, this is the URL we were redirected to.
    updated_url: Option<Url>,
    /// Headers to include in the request.
    headers: HeaderMap,
    /// Information we've been given about the remote file.
    user_provided_remote_file_info: FileInfo,
    /// The file size of the remote file, if we work it out before downloading.
    /// This will only get filled in if we need to do a HEAD request to work out
    /// the filename.
    remote_file_length: Option<u64>,
    /// The configured destination for the file, if any.  This could be a directory
    /// or an actual file.
    destination: Option<PathBuf>,
    /// The maximum number of times we will consecutively retry without making progress.
    max_retries: u64,
    /// The callback to call to report progress.
    progress_handler: Option<Box<dyn Progress>>,
    /// The handler to call when we retry a download.
    retry_handler: RetryHandler,
    /// If there are any errors while configuring the download, we store them here,
    /// so we can return them when we actually try to start the download.
    err: Option<Error>,
    // TODO: Rate limiting
}

struct DownloadInner {
    /// The client to use to download the file.
    client: reqwest::Client,
    /// Headers to include in the request.
    headers: HeaderMap,
    /// Information we know about the remote file, either from the user, from
    /// the sidecar file, or if we're fetching the file from scratch, from the server.
    remote_file_info: FileInfo,
    /// The maximum number of times we can consecutively retry without making any progress.
    max_retries: u64,
    /// Progress callback, if any.
    progress_handler: Option<Box<dyn Progress>>,
    /// The handler to call when we retry a download.
    retry_handler: RetryHandler,
    /// Temporary file we'll write while we're downloading (e.g. "file.txt.part")
    part_filename: PathBuf,
    ///  "Sidecar" file where we'll store info about the file (the etag, the last modified, etc...).  (e.g. "file.txt.downloadinfo")
    sidecar_filename: PathBuf,
    /// File we're writing to.
    part_file: File,
    /// Current size of the local file.
    local_file_size: u64,
}

#[derive(Debug, Clone)]
pub struct DownloadResult {
    pub status: Status,
    pub tries: u64,
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
            user_provided_remote_file_info: FileInfo::default(),
            destination: None,
            max_retries: DEFAULT_MAX_RETRIES,
            progress_handler: None,
            retry_handler: Box::new(default_retry_callback),
            headers: HeaderMap::new(),
            remote_file_length: None,
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
    pub fn on_progress(mut self, progress: impl FnMut(&mut ProgressHandle) + 'static) -> Self {
        self.progress_handler = Some(Box::new(progress));
        self
    }

    /// Provide a callback to be called whenever a download is retried. This can
    /// be used to customize the retry time, or abort the download. The default
    /// is to use exponential backoff:
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use std::time::Duration;
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    ///     let client = downlow::Client::new();
    ///     let result = client.download("https://example.com/file.txt")
    ///        .destination("file.txt")
    ///        .on_retry(|r| {
    ///           if matches!(r.error(), downlow::Error::FileChanged { .. }) {
    ///               // No delay if the file changed.
    ///               r.set_delay(Duration::ZERO);
    ///           } else {
    ///               r.set_delay(downlow::exponential_backoff(
    ///                   Duration::from_millis(500),
    ///                   Duration::from_secs(10),
    ///                   r.retries(),
    ///               ));
    ///           }
    ///         })
    ///        .download()
    ///        .await?;
    /// #   Ok(())
    /// # }
    /// ```
    ///
    /// This can also be used to abort the download on a retry by calling `r.cancel()`:
    ///
    /// ```no_run
    /// # use std::time::Duration;
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    ///     let client = downlow::Client::new();
    ///     let result = client.download("https://example.com/i_do_not_exist.txt")
    ///        .destination("file.txt")
    ///        .on_retry(|r| {
    ///           r.cancel();
    ///         })
    ///        .download()
    ///        .await;
    ///
    ///     assert!(matches!(result, Err(downlow::Error::UnexpectedStatus { status: 404, .. })));
    /// #   Ok(())
    /// # }
    /// ```
    ///
    pub fn on_retry(mut self, retry: impl FnMut(&mut RetryHandle) + 'static) -> Self {
        self.retry_handler = Box::new(retry);
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
        self.user_provided_remote_file_info.etag = Some(etag.into());
        self
    }

    /// Set the last modified time for this file.  If you have already downloaded
    /// part of the file and know the last modified time, setting this will allow
    /// the download to verify that the file has not changed on the server before
    /// resuming.  If neither this nor the etag are set, then the mtime of the existing
    /// file on disk will be used in place of the last modified time.
    pub fn last_modified(mut self, last_modified: SystemTime) -> Self {
        self.user_provided_remote_file_info.modified = Some(last_modified.into());
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
            let remote_file_length = match self.remote_file_length {
                Some(v) => Some(v),
                None => {
                    self.get_remote_file_length(self.url.clone(), self.headers.clone())
                        .await
                }
            };
            if let Some(remote_length) = remote_file_length {
                if metadata.len() == remote_length {
                    // File already exists and is the correct length - nothing to do.
                    return Ok(DownloadResult {
                        status: Status::Skipped,
                        tries: 0,
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

        let mut remote_file_info = self.user_provided_remote_file_info;
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

        let progress_data = ProgressHandle {
            original_url: self.url,
            updated_url: self.updated_url,
            destination,
            tries: 0,
            bytes_transferred: 0,
            bytes: local_file_size,
            total_bytes: remote_file_info.length,
            cancelled: false,
        };

        let inner = DownloadInner {
            client: self.client,
            headers: self.headers,
            remote_file_info,
            max_retries: self.max_retries,
            progress_handler: self.progress_handler,
            retry_handler: self.retry_handler,
            part_filename,
            sidecar_filename,
            part_file,
            local_file_size,
        };

        inner.download(progress_data).await
    }

    async fn get_remote_file_length(&mut self, url: Url, headers: HeaderMap) -> Option<u64> {
        let (url, head) = request(&self.client, Method::HEAD, url, headers).await;
        if url.is_some() {
            // Update the URL we fetched from.
            self.updated_url = url;
        }
        match head {
            Ok(head) => {
                if head.status().is_success() {
                    headers::parse_content_length(&head)
                } else {
                    None
                }
            }
            Err(_) => None,
        }
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
            let filename = match &head {
                Ok(head) => {
                    if head.status().is_success() {
                        // While we're here, see if we can work out the file length.
                        self.remote_file_length = headers::parse_content_length(head);

                        // Work out the filename from the Content-Disposition header, if present.
                        headers::parse_content_disposition(head)
                    } else {
                        None
                    }
                }
                Err(_) => None,
            };

            let filename = filename
                .or_else(|| {
                    let url_filename = url.path().split('/').next_back().unwrap();
                    if url_filename.is_empty() {
                        None
                    } else {
                        Some(Cow::Borrowed(url_filename))
                    }
                })
                .unwrap_or(Cow::Borrowed("file"));

            destination = Cow::Owned(destination.as_ref().join(filename.as_ref()));
        };

        Ok(destination.into_owned())
    }
}

impl DownloadInner {
    async fn download(
        mut self,
        mut progress_data: ProgressHandle,
    ) -> Result<DownloadResult, Error> {
        let mut retries = 0;

        let mut done = false;
        while !done {
            progress_data.tries += 1;
            retries += 1;

            let bytes_before = progress_data.bytes_transferred;
            match self.try_download(&mut progress_data).await {
                Ok(()) => {
                    done = true;
                }
                Err(e) => {
                    if !e.can_retry() {
                        return Err(e);
                    } else {
                        if progress_data.bytes_transferred > bytes_before {
                            // We made some progress - reset the retry counter.
                            retries = 0;
                        } else if retries > self.max_retries {
                            return Err(e);
                        }

                        let mut delay = time::exponential_backoff(
                            DEFAULT_MIN_DELAY,
                            DEFAULT_MAX_DELAY,
                            retries,
                        );

                        if matches!(e, Error::FileChanged { .. }) {
                            // The file has changed on the server - we need to start again.
                            self.truncate().await?;
                            self.remote_file_info
                                .update(&self.sidecar_filename, None, None, None)
                                .await;
                            delay = Duration::from_secs(0);
                        }

                        let mut retry_handle = RetryHandle {
                            total_tries: progress_data.tries,
                            retries,
                            delay,
                            error: e,
                            cancelled: false,
                        };
                        (self.retry_handler)(&mut retry_handle);
                        if retry_handle.cancelled {
                            return Err(retry_handle.error);
                        }
                        tokio::time::sleep(retry_handle.delay).await;
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
            &progress_data.destination,
            self.remote_file_info.modified.as_ref(),
        )
        .await?;

        notify(&mut self.progress_handler, &mut progress_data)?;

        Ok(DownloadResult {
            status: Status::Downloaded,
            tries: progress_data.tries,
            path: progress_data.destination,
            bytes_downloaded: progress_data.bytes_transferred,
        })
    }

    /// This is the "inner loop" of the download. Try to download the file, and return
    /// an error if it fails for any reason.  The caller can then decide whether to retry or not.
    async fn try_download(&mut self, progress_data: &mut ProgressHandle) -> Result<(), Error> {
        // Make our GET request.
        let response = self.get_file(progress_data).await?;

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
            // If we're not resuming, then update our info about the remote file.
            self.remote_file_info
                .update(&self.sidecar_filename, content_length, last_modified, etag)
                .await;
        }

        // Copy data from the response to the .part file.
        self.copy_response_to_file(progress_data, response, append)
            .await?;

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
    async fn get_file(&mut self, progress_data: &mut ProgressHandle) -> Result<Response, Error> {
        let mut headers = self.headers.clone();
        self.add_resume_download_headers(&mut headers);
        let url = progress_data.url();
        let (u, response) = request(&self.client, reqwest::Method::GET, url.clone(), headers).await;
        if u.is_some() {
            progress_data.updated_url = u
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
        progress_data: &mut ProgressHandle,
        mut response: Response,
        append: bool,
    ) -> Result<u64, Error> {
        let mut bytes_downloaded = 0;
        if !append {
            self.truncate().await?;
        }
        let initial_size = self.local_file_size;

        progress_data.bytes = self.local_file_size;
        progress_data.total_bytes = response
            .content_length()
            .map(|v| v + initial_size)
            .or(self.remote_file_info.length);

        // Initial call into the progress callback.
        notify(&mut self.progress_handler, progress_data)?;

        loop {
            let chunk_result = response.chunk().await.map_err(|cause| Error::Network {
                during: "read",
                url: progress_data.url().to_string(),
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
            progress_data.bytes_transferred += chunk_size;
            progress_data.bytes = self.local_file_size;
            notify(&mut self.progress_handler, progress_data)?;
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
            Err(Error::UnexpectedStatus {
                status: response.status().as_u16(),
            }),
        );
    }

    (returned_url, Ok(response))
}

fn notify(
    progress: &mut Option<Box<dyn Progress>>,
    progress_data: &mut ProgressHandle,
) -> Result<(), Error> {
    if let Some(progress) = progress.as_mut() {
        progress.progress(progress_data);
    }

    if progress_data.cancelled {
        return Err(Error::Cancelled);
    }

    Ok(())
}
