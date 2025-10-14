use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use http::{HeaderMap, HeaderValue, header::IntoHeaderName};
use reqwest::Response;
use tokio::{fs::File, io::AsyncWriteExt};
use url::Url;

use crate::{
    DEFAULT_MAX_DELAY, DEFAULT_MIN_DELAY, DownloadResult, Error, IntoUrl, Progress, ProgressHandle,
    RetryHandle, RetryHandler,
    destination::Destination,
    file_info::FileInfo,
    head::Head,
    headers,
    limiter::TokioLimiter,
    utils::{self, http::append_header},
};

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
    /// Information about the remote file, if we need to retrieve it.
    head: Option<Head>,
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
    progress: ProgressHandle,
}

/// Default callback used for determining backoff delay between retries.
fn default_retry_callback(handle: &mut RetryHandle) {
    if matches!(handle.error(), Error::FileChanged { .. }) {
        // No delay if the file changed.
        handle.set_delay(Duration::ZERO);
    } else {
        handle.set_delay(crate::exponential_backoff(
            DEFAULT_MIN_DELAY,
            DEFAULT_MAX_DELAY,
            handle.retries(),
        ));
    }
}

impl Download {
    /// Create a new download for the given URL.
    pub(crate) fn create(
        client: reqwest::Client,
        max_retries: Option<u64>,
        limiter: Arc<TokioLimiter>,
        url: impl IntoUrl,
    ) -> Self {
        let (url, err) = match url.into_url() {
            Ok(u) => (u, None),
            Err(e) => (Url::parse("http://invalid/").unwrap(), Some(e)),
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
            head: None,
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
    ///     let client = downlowd::Client::new();
    ///     let result = client.download("https://example.com/file.txt")
    ///        .destination("file.txt")
    ///        .on_retry(|r| {
    ///           if matches!(r.error(), downlowd::Error::FileChanged { .. }) {
    ///               // No delay if the file changed.
    ///               r.set_delay(Duration::ZERO);
    ///           } else {
    ///               r.set_delay(downlowd::exponential_backoff(
    ///                   Duration::from_secs(5),
    ///                   Duration::from_secs(120),
    ///                   r.retries(),
    ///               ));
    ///           }
    ///         })
    ///        .send()
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
    ///     let client = downlowd::Client::new();
    ///     let result = client.download("http://localhost:8089/i_do_not_exist.txt")
    ///        .destination("file.txt")
    ///        .on_retry(|r| r.cancel())
    ///        .send()
    ///        .await;
    ///
    ///     assert!(matches!(result, Err(downlowd::Error::UnexpectedStatus { status: 404, .. })));
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

    async fn head(&mut self) -> Result<&Head, Error> {
        if self.head.is_none() {
            let head = Head::create(&self.client, &self.url, self.headers.clone())
                .await
                .unwrap_or_default();
            self.head = Some(head);
        }
        Ok(self.head.as_ref().unwrap())
    }

    /// Returns the filename that downlowd will use when downloading the file.
    /// This is determined by making a HEAD request to the server, and looking
    /// at the `Content-Disposition` header, if present, or falling back to the
    /// last part of the URL path.
    pub async fn get_remote_file_name(&mut self) -> &str {
        let head = self.head().await;
        head.ok()
            .and_then(|h| h.filename.as_deref())
            .unwrap_or("file")
    }

    /// Try to get the length of the remote file.  This may return None if the
    /// server doesn't provide a Content-Length header.
    async fn get_remote_file_length(&mut self) -> Option<u64> {
        let head = self.head().await;
        head.ok()
            .and_then(|h| h.remote_file_info.as_ref())
            .and_then(|info| info.file_length)
    }

    /// Send the download request to the server.
    pub async fn send(mut self) -> Result<DownloadResult, Error> {
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

        let inner = DownloadInner::new(self, destination).await?;
        inner.download().await
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
    async fn new(dl: Download, destination: Destination) -> Result<Self, Error> {
        // Use information provided by the user, or else load from the sidecar file if it exists.
        let mut local_file_info = dl.user_provided_local_file_info;
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
        let progress = ProgressHandle::new(
            dl.url,
            dl.head.and_then(|h| h.updated_url),
            destination,
            local_file_info,
            file_length,
        );

        Ok(Self {
            client: dl.client,
            limiter: dl.limiter,
            headers: dl.headers,
            max_retries: dl.max_retries,
            progress_handler: dl.progress_handler,
            retry_handler: dl.retry_handler,
            part_file,
            progress,
        })
    }

    async fn download(mut self) -> Result<DownloadResult, Error> {
        let mut retries = 0;

        loop {
            if self.progress.is_complete() == Some(true) {
                // We already have the whole file!
                break;
            }

            self.progress.tries += 1;
            retries += 1;

            let bytes_before = self.progress.bytes_transferred;
            match self.try_download().await {
                Ok(()) => break,
                Err(e) => {
                    if !e.can_retry() {
                        return Err(e);
                    } else {
                        if self.progress.bytes_transferred > bytes_before {
                            // We made some progress - reset the retry counter.
                            retries = 0;
                        }
                        if let Some(max_retries) = self.max_retries
                            && retries > max_retries
                        {
                            return Err(e);
                        }

                        // Set a default delay, in case the retry handler doesn't.
                        let delay = if matches!(e, Error::FileChanged { .. }) {
                            // The file has changed on the server - we need to start again.
                            utils::file::truncate_file_async(
                                &self.progress.destination.part_file,
                                &mut self.part_file,
                            )
                            .await?;
                            self.progress.reset().await;
                            Duration::from_secs(0)
                        } else {
                            crate::exponential_backoff(
                                DEFAULT_MIN_DELAY,
                                DEFAULT_MAX_DELAY,
                                retries,
                            )
                        };

                        let mut retry_handle =
                            RetryHandle::new(self.progress.tries, retries, delay, e);
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
        tokio::fs::rename(
            &self.progress.destination.part_file,
            &self.progress.destination.path,
        )
        .await
        .map_err(|e| Error::Write {
            action: "renaming part file",
            path: self.progress.destination.part_file,
            cause: e,
        })?;

        // Delete the sidecar file.
        let _ = tokio::fs::remove_file(&self.progress.destination.sidecar_file).await;

        Ok(DownloadResult {
            tries: self.progress.tries,
            path: self.progress.destination.path,
            bytes_downloaded: self.progress.bytes_transferred,
        })
    }

    /// This is the "inner loop" of the download. Try to download the file, and return
    /// an error if it fails for any reason.  The caller can then decide whether to retry or not.
    async fn try_download(&mut self) -> Result<(), Error> {
        // Make our GET request.
        let response = self.get_file().await?;

        if response.status().as_u16() == 416 {
            // The server thinks the range we requested is not satisfiable. Nginx will return this if, for example,
            // we have the whole file already and we're effectively asking for zero bytes.
            if let Some(total) =
                headers::parse_content_range(response.headers()).and_then(|cr| cr.total)
                && self.progress.bytes == total
            {
                // We already have the whole file!
                return Ok(());
            } else {
                // We don't have the whole file, but the server says it can't
                // give us more?
                return Err(Error::FileChanged {
                    description: "range not satisfiable",
                });
            }
        }

        // If the server returns a "206 - Partial content", we're resuming the download,
        // so we should append to the existing file.  Otherwise, we should overwrite it.
        let append = response.status().as_u16() == 206; // Partial content
        let remote_file_info = FileInfo::from_response(
            response.status().as_u16(),
            response.headers(),
            self.progress.bytes,
        );

        if append {
            // If we're trying to append to an existing file, but the file has changed on
            // the server, then error.  This SHOULD never happen, thanks to the `If-Range`
            // header we sent, but some servers are not well behaved.
            self.progress
                .local_file_info
                .verify_unchanged(&remote_file_info)?;
        }
        self.progress.local_file_info = remote_file_info;
        self.progress
            .local_file_info
            .save(&self.progress.destination.sidecar_file)
            .await;

        // Copy data from the response to the .part file.
        let result = self.copy_response_to_file(response, append).await;

        // Flush the file to ensure all data is written before we return.
        let _ = self.part_file.flush().await;
        result?;

        Ok(())
    }

    /// Work out the range headers to use to resume the download.
    fn add_resume_download_headers(&self, headers: &mut HeaderMap) {
        if self.progress.bytes > 0 {
            let last_modified = self.progress.local_file_info.last_modified.as_deref();
            let etag = self.progress.local_file_info.etag.as_deref();

            if let Some(if_range) = etag.or(last_modified) {
                headers.insert(
                    "Range",
                    HeaderValue::from_str(&format!("bytes={}-", self.progress.bytes)).unwrap(),
                );
                headers.insert("If-Range", HeaderValue::from_str(if_range).unwrap());
            }
        }
    }

    /// Send a GET request for the file.
    async fn get_file(&mut self) -> Result<Response, Error> {
        let mut headers = self.headers.clone();
        self.add_resume_download_headers(&mut headers);
        let url = self.progress.url();
        let (u, response) =
            utils::reqwest::request(&self.client, reqwest::Method::GET, url.clone(), headers).await;
        if u.is_some() {
            self.progress.updated_url = u
        }

        if let Ok(response) = response.as_ref()
            && !response.status().is_success()
            && response.status().as_u16() != 416
        {
            return Err(Error::UnexpectedStatus {
                status: response.status().as_u16(),
            });
        }

        response
    }

    /// Stream data from the response to a file, and call into the progress callback as we go.
    /// Returns the total number of bytes written to the file, whether or not this succeeds.
    async fn copy_response_to_file(
        &mut self,
        mut response: Response,
        append: bool,
    ) -> Result<u64, Error> {
        // The number of bytes downloaded on this attempt.
        let mut bytes_downloaded = 0;

        if !append {
            utils::file::truncate_file_async(
                &self.progress.destination.part_file,
                &mut self.part_file,
            )
            .await?;
            self.progress.bytes = 0;
        }

        // Initial call into the progress callback.
        notify(&mut self.progress_handler, &mut self.progress)?;

        while let Some(chunk) = response.chunk().await.map_err(|cause| Error::Network {
            during: "read",
            url: self.progress.url().to_string(),
            cause: cause.without_url().to_string(),
        })? {
            self.part_file
                .write_all(&chunk)
                .await
                .map_err(|err| Error::Write {
                    action: "writing to file",
                    path: self.progress.destination.part_file.clone(),
                    cause: err,
                })?;

            let chunk_size = chunk.len() as u64;
            bytes_downloaded += chunk_size;
            self.progress.delta = chunk_size;
            self.progress.bytes += chunk_size;
            self.progress.bytes_transferred += chunk_size;
            notify(&mut self.progress_handler, &mut self.progress)?;

            // Let the rate limiter know we downloaded some bytes.
            self.limiter
                .bytes_consumed(
                    chunk.len() as u64,
                    self.progress.is_complete().unwrap_or_default(),
                )
                .await;
        }

        Ok(bytes_downloaded)
    }
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
