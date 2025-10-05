#![doc = include_str!("../README.md")]

use std::{
    borrow::Cow,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

mod backoff;
mod client;
mod destination;
mod error;
mod file_info;
mod handles;
mod headers;
mod limiter;
mod utils;

#[cfg(test)]
mod tests;

use http::Method;
use reqwest::Response;
use tokio::{fs::File, io::AsyncWriteExt};
use url::Url;

use crate::{
    destination::Destination, file_info::FileInfo, limiter::TokioLimiter,
    utils::http::append_header,
};

pub use backoff::exponential_backoff;
pub use client::{Client, ClientBuilder};
pub use error::Error;
pub use handles::*;
pub use http::{HeaderMap, HeaderValue, header::IntoHeaderName};
pub use utils::into_url::IntoUrl;

const DEFAULT_MIN_DELAY: Duration = Duration::from_secs(1);
const DEFAULT_MAX_DELAY: Duration = Duration::from_secs(120);

/// Represents a file about to be downloaded.
pub struct Download {
    /// The client to use to download the file.
    client: reqwest::Client,
    /// Rate limiter.
    limiter: Arc<TokioLimiter>,
    /// The URL we want to download from.
    url: Url,
    /// Headers to include in the request.
    headers: HeaderMap,
    /// The configured destination for the file, if any.  This could be a directory
    /// or an actual file.
    destination: Option<PathBuf>,
    /// The maximum number of times we will consecutively retry without making progress.
    max_retries: Option<u64>,
    /// The callback to call to report progress.
    progress_handler: Option<Box<dyn Progress + Send>>,
    /// The handler to call when we retry a download.
    retry_handler: RetryHandler,
    /// Information we've been given about the remote file.
    user_provided_local_file_info: FileInfo,

    /// If there are any errors while configuring the download, we store them here,
    /// so we can return them when we actually try to start the download.
    err: Option<Error>,
    /// If we are redirected, this is the URL we were redirected to.
    updated_url: Option<Url>,
    /// The name of the remote file, which will be filled in if we HEAD the file.
    remote_file_name: Option<String>,
    /// Information about the remote file, which will be filled in if we HEAD the file.
    remote_file_info: Option<FileInfo>,
}

struct DownloadInner {
    /// The client to use to download the file.
    client: reqwest::Client,
    /// Rate limiter.
    limiter: Arc<TokioLimiter>,
    /// Headers to include in the request.
    headers: HeaderMap,
    /// The maximum number of times we can consecutively retry without making any progress.
    max_retries: Option<u64>,
    /// Progress callback, if any.
    progress_handler: Option<Box<dyn Progress + Send>>,
    /// The handler to call when we retry a download.
    retry_handler: RetryHandler,
    /// File we're writing to.
    part_file: File,
}

/// Default callback used for determining backoff delay between retries.
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

impl Download {
    /// Create a new download for the given URL.
    fn create(
        client: reqwest::Client,
        max_retries: Option<u64>,
        limiter: Arc<TokioLimiter>,
        url: impl IntoUrl,
    ) -> Self {
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
            limiter,
            url,
            headers: HeaderMap::new(),
            destination: None,
            max_retries,
            progress_handler: None,
            retry_handler: Box::new(default_retry_callback),
            user_provided_local_file_info: FileInfo::default(),

            err,
            updated_url: None,
            remote_file_name: None,
            remote_file_info: None,
        }
    }

    /// Set the user agent for this download.
    pub fn user_agent(self, user_agent: impl Into<String>) -> Self {
        self.header(http::header::USER_AGENT, user_agent.into())
    }

    /// Add a header to this download.
    pub fn header<K, V>(mut self, key: K, value: V) -> Self
    where
        K: IntoHeaderName,
        HeaderValue: TryFrom<V>,
        <HeaderValue as TryFrom<V>>::Error: Into<http::Error>,
    {
        if let Err(e) = append_header(&mut self.headers, key, value) {
            self.err = Some(e);
        }

        self
    }

    /// Add a set of Headers to the existing ones on this download.
    /// The headers will be merged in to any already set.
    pub fn headers(mut self, headers: HeaderMap) -> Self {
        utils::http::append_all_headers(&mut self.headers, headers);
        self
    }

    /// Override the the maxmimum number of times to consecutively retry the
    /// download without making any progress. Pass in `None` to retry forever.
    pub fn max_retries(mut self, max_retries: Option<u64>) -> Self {
        self.max_retries = max_retries;
        self
    }

    /// Set the progress reporter for this download.  The given reporter will
    /// be called periodically as data is downloaded.
    pub fn on_progress(
        mut self,
        progress: impl FnMut(&mut ProgressHandle) + Send + 'static,
    ) -> Self {
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
    ///                   Duration::from_secs(5),
    ///                   Duration::from_secs(120),
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
    /// ```
    /// # use std::time::Duration;
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    ///     let client = downlow::Client::new();
    ///     let result = client.download("http://localhost:8089/i_do_not_exist.txt")
    ///        .destination("file.txt")
    ///        .on_retry(|r| r.cancel())
    ///        .download()
    ///        .await;
    ///
    ///     assert!(matches!(result, Err(downlow::Error::UnexpectedStatus { status: 404, .. })));
    /// #   Ok(())
    /// # }
    /// ```
    ///
    pub fn on_retry(mut self, retry: impl FnMut(&mut RetryHandle) + Send + 'static) -> Self {
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
        self.user_provided_local_file_info.etag = Some(etag.into());
        self
    }

    /// Set the last modified time for this file.  If you have already downloaded
    /// part of the file and know the last modified time, setting this will allow
    /// the download to verify that the file has not changed on the server before
    /// resuming.  If neither this nor the etag are set, then the mtime of the existing
    /// file on disk will be used in place of the last modified time.
    pub fn last_modified(mut self, last_modified: impl Into<String>) -> Self {
        self.user_provided_local_file_info.last_modified = Some(last_modified.into());
        self
    }

    /// Returns the filename that downlow will use when downloading the file.
    /// This is determined by making a HEAD request to the server, and looking
    /// at the `Content-Disposition` header, if present, or falling back to the
    /// last part of the URL path.
    pub async fn get_remote_file_name(&mut self) -> String {
        if let Some(name) = &self.remote_file_name {
            return name.clone();
        }

        let (u, head) = request(
            &self.client,
            reqwest::Method::HEAD,
            self.url.clone(),
            self.headers.clone(),
        )
        .await;

        // If we followed a redirect, take note of this, so we don't have to follow
        // it again.
        if u.is_some() {
            self.updated_url = u;
        }

        // This is the URL we actually downloaded from.
        let url = self.updated_url.as_ref().unwrap_or(&self.url);

        // Get the filename from the server.
        let filename = head
            .ok()
            .and_then(|head| {
                if head.status().is_success() {
                    self.remote_file_info = Some(FileInfo::from_reqwest_response(&head, 0));
                    // Work out the filename from the Content-Disposition header, if present.
                    headers::parse_content_disposition(&head).map(Cow::<str>::into_owned)
                } else {
                    None
                }
            })
            .or_else(|| {
                let url_filename = url.path().split('/').next_back().unwrap();
                if url_filename.is_empty() {
                    None
                } else {
                    Some(url_filename.to_owned())
                }
            })
            .unwrap_or("file".to_string());

        // Cache this so we don't need to get it again.
        self.remote_file_name = Some(filename.clone());

        filename
    }

    /// This causes the file to actually be downloaded.
    pub async fn download(mut self) -> Result<DownloadResult, Error> {
        if let Some(e) = self.err {
            return Err(e);
        }

        // Work out where we're ultimately going to save the file.
        let destination = self.resolve_destination().await?;

        // TODO: Allow forcing the download if the file already exists.
        if self.should_skip(&destination.path).await {
            // File already exists and is the correct length - nothing to do.
            return Ok(DownloadResult {
                tries: 0,
                path: destination.path,
                bytes_downloaded: 0,
            });
        }

        // Use information provided by the user, or else load from the sidecar file if it exists.
        let mut local_file_info = self.user_provided_local_file_info;
        if local_file_info.etag.is_none() && local_file_info.last_modified.is_none() {
            // User didn't tell us anything...
            let _ = local_file_info.load(&destination.sidecar_file).await;
        }

        // Open the part file for writing.
        let mut part_file =
            utils::file::open_file_for_writing_async(&destination.part_file).await?;
        let file_length =
            utils::file::get_file_length_async(&mut part_file, &destination.part_file).await?;

        // This is the single instance of `ProgressHandle` that we'll update
        // and pass to the progress handler throughout the download.
        let progress = ProgressHandle {
            original_url: self.url,
            updated_url: self.updated_url,
            destination,
            tries: 0,
            bytes_transferred: 0,
            bytes: file_length,
            delta: 0,
            local_file_info,
            cancelled: false,
        };

        let inner = DownloadInner {
            client: self.client,
            limiter: self.limiter,
            headers: self.headers,
            max_retries: self.max_retries,
            progress_handler: self.progress_handler,
            retry_handler: self.retry_handler,
            part_file,
        };

        inner.download(progress).await
    }

    /// Returns true if the local file exists and is the same length as the remote file.
    async fn should_skip(&mut self, destination: &Path) -> bool {
        let local_length = tokio::fs::metadata(&destination)
            .await
            .ok()
            .map(|m| m.len());

        if let Some(local_length) = local_length
            && let Some(remote_length) = self.get_remote_file_length().await
        {
            return local_length == remote_length;
        }

        false
    }

    /// Try to get the length of the remote file.  This may return None if the
    /// server doesn't provide a Content-Length header.
    async fn get_remote_file_length(&mut self) -> Option<u64> {
        let remote_file_length = self
            .remote_file_info
            .as_ref()
            .and_then(|info| info.file_length);
        match remote_file_length {
            Some(v) => Some(v),
            None => {
                let (url, head) = request(
                    &self.client,
                    Method::HEAD,
                    self.url.clone(),
                    self.headers.clone(),
                )
                .await;

                if url.is_some() {
                    // Update the URL we fetched from.
                    self.updated_url = url;
                }

                head.ok().and_then(|head| {
                    if head.status().is_success() {
                        let remote_file_info = FileInfo::from_reqwest_response(&head, 0);
                        let result = remote_file_info.file_length;
                        self.remote_file_info = Some(remote_file_info);
                        result
                    } else {
                        None
                    }
                })
            }
        }
    }

    /// Returns the final destination path for the download.
    async fn resolve_destination(&mut self) -> Result<Destination, Error> {
        let mut destination = match &self.destination {
            Some(path) => path.as_path().to_owned(),
            None => std::env::current_dir().map_err(|e| Error::Write {
                action: "determining current directory",
                path: PathBuf::from("."),
                cause: e,
            })?,
        };

        // If the destination is a directory, figure out the filename for the file.
        if utils::file::is_dir_async(&destination).await {
            let filename = self.get_remote_file_name().await;
            destination = destination.join(filename);
        };

        Ok(Destination::new(destination))
    }
}

impl DownloadInner {
    async fn download(mut self, mut progress: ProgressHandle) -> Result<DownloadResult, Error> {
        let mut retries = 0;

        let mut done = false;
        while !done {
            progress.tries += 1;
            retries += 1;

            let bytes_before = progress.bytes_transferred;
            match self.try_download(&mut progress).await {
                Ok(()) => {
                    done = true;
                }
                Err(e) => {
                    if !e.can_retry() {
                        return Err(e);
                    } else {
                        if progress.bytes_transferred > bytes_before {
                            // We made some progress - reset the retry counter.
                            retries = 0;
                        } else if let Some(max_retries) = self.max_retries
                            && retries > max_retries
                        {
                            return Err(e);
                        }

                        let mut delay =
                            exponential_backoff(DEFAULT_MIN_DELAY, DEFAULT_MAX_DELAY, retries);

                        if matches!(e, Error::FileChanged { .. }) {
                            // The file has changed on the server - we need to start again.
                            utils::file::truncate_file_async(
                                &progress.destination.part_file,
                                &mut self.part_file,
                            )
                            .await?;
                            progress.bytes = 0;
                            // Reset the local file info. It'll get filled in again
                            // at the start of the next download attempt.
                            progress
                                .local_file_info
                                .reset(&progress.destination.sidecar_file)
                                .await;
                            delay = Duration::from_secs(0);
                        }

                        let mut retry_handle = RetryHandle {
                            total_tries: progress.tries,
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

        // Rename the .part file to the final file.
        tokio::fs::rename(&progress.destination.part_file, &progress.destination.path)
            .await
            .map_err(|e| Error::Write {
                action: "renaming part file",
                path: progress.destination.part_file,
                cause: e,
            })?;

        // Delete the sidecar file.
        let _ = tokio::fs::remove_file(&progress.destination.sidecar_file).await;

        Ok(DownloadResult {
            tries: progress.tries,
            path: progress.destination.path,
            bytes_downloaded: progress.bytes_transferred,
        })
    }

    /// This is the "inner loop" of the download. Try to download the file, and return
    /// an error if it fails for any reason.  The caller can then decide whether to retry or not.
    async fn try_download(&mut self, progress: &mut ProgressHandle) -> Result<(), Error> {
        // Make our GET request.
        let response = self.get_file(progress).await?;

        // If the server returns a "206 - Partial content", we're resuming the download,
        // so we should append to the existing file.  Otherwise, we should overwrite it.
        let append = response.status().as_u16() == 206; // Partial content
        let remote_file_info = FileInfo::from_reqwest_response(&response, progress.bytes);

        if append {
            // If we're trying to append to an existing file, but the file has changed on
            // the server, then error.  This SHOULD never happen, thanks to the `If-Range`
            // header we sent, but some servers are not well behaved.
            progress
                .local_file_info
                .verify_unchanged(&remote_file_info)?;
        }
        progress.local_file_info = remote_file_info;
        progress
            .local_file_info
            .save(&progress.destination.sidecar_file)
            .await;

        // Copy data from the response to the .part file.
        let result = self.copy_response_to_file(progress, response, append).await;

        // Flush the file to ensure all data is written before we return.
        let _ = self.part_file.flush().await;
        result?;

        Ok(())
    }

    /// Work out the range headers to use to resume the download.
    fn add_resume_download_headers(&self, progress: &ProgressHandle, headers: &mut HeaderMap) {
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

    /// Send a GET request for the file.
    async fn get_file(&mut self, progress: &mut ProgressHandle) -> Result<Response, Error> {
        let mut headers = self.headers.clone();
        self.add_resume_download_headers(progress, &mut headers);
        let url = progress.url();
        let (u, response) = request(&self.client, reqwest::Method::GET, url.clone(), headers).await;
        if u.is_some() {
            progress.updated_url = u
        }
        response
    }

    /// Stream data from the response to a file, and call into the progress callback as we go.
    /// Returns the total number of bytes written to the file, whether or not this succeeds.
    async fn copy_response_to_file(
        &mut self,
        progress: &mut ProgressHandle,
        mut response: Response,
        append: bool,
    ) -> Result<u64, Error> {
        // The number of bytes downloaded on this attempt.
        let mut bytes_downloaded = 0;

        if !append {
            utils::file::truncate_file_async(&progress.destination.part_file, &mut self.part_file)
                .await?;
            progress.bytes = 0;
        }

        // Initial call into the progress callback.
        notify(&mut self.progress_handler, progress)?;

        while let Some(chunk) = response.chunk().await.map_err(|cause| Error::Network {
            during: "read",
            url: progress.url().to_string(),
            cause,
        })? {
            // Let the rate limiter know we downloaded some bytes.
            self.limiter.bytes_consumed(chunk.len() as u64).await;

            self.part_file
                .write_all(&chunk)
                .await
                .map_err(|err| Error::Write {
                    action: "writing to file",
                    path: progress.destination.part_file.clone(),
                    cause: err,
                })?;

            let chunk_size = chunk.len() as u64;
            bytes_downloaded += chunk_size;
            progress.delta = chunk_size;
            progress.bytes += chunk_size;
            progress.bytes_transferred += chunk_size;
            notify(&mut self.progress_handler, progress)?;
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
    progress_handler: &mut Option<Box<dyn Progress + Send>>,
    progress: &mut ProgressHandle,
) -> Result<(), Error> {
    if let Some(handler) = progress_handler.as_mut() {
        handler.progress(progress);
    }

    if progress.cancelled {
        return Err(Error::Cancelled);
    }

    Ok(())
}
