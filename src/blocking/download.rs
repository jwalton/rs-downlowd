use std::{
    fs::{self, File},
    io::{Read, Write},
    path::Path,
    sync::Arc,
    thread,
    time::Duration,
};

use http::{HeaderMap, HeaderValue, Response, StatusCode, header::IntoHeaderName};
use ureq::Body;

use crate::{
    DEFAULT_MAX_DELAY, DEFAULT_MIN_DELAY, DownloadResult, Error, Progress, ProgressHandle,
    RetryHandle, RetryHandler,
    blocking::{tokenbucket::BlockingTokenBucket, ureq_utils},
    destination::Destination,
    file_info::FileInfo,
    head::Head,
    headers::{self, add_resume_download_headers},
    shared::DownloadConfig,
    utils,
};

const BUFFER_SIZE: usize = 128 * 1024;

/// Represents a file about to be downloaded.
pub struct Download {
    /// The client to use to download the file.
    agent: ureq::Agent,
    /// Rate limiter.
    limiter: Arc<BlockingTokenBucket>,
    /// How do we want to download this file?
    config: DownloadConfig,
    /// Information about the remote file, if we need to retrieve it.
    head: Option<Head>,
}

struct DownloadInner {
    /// The ureq agent to use to download the file.
    agent: ureq::Agent,
    /// Rate limiter.
    limiter: Arc<BlockingTokenBucket>,
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

impl Download {
    /// Create a new download for the given URL.
    pub(crate) fn new(
        agent: ureq::Agent,
        limiter: Arc<BlockingTokenBucket>,
        config: DownloadConfig,
    ) -> Self {
        Download {
            agent,
            limiter,
            config,
            head: None,
        }
    }

    /// Set the user agent for this download.
    pub fn user_agent(mut self, user_agent: impl Into<String>) -> Self {
        self.config.user_agent(user_agent);
        self
    }

    /// Add a header to this download.
    pub fn header<K, V>(mut self, key: K, value: V) -> Self
    where
        K: IntoHeaderName,
        HeaderValue: TryFrom<V>,
        <HeaderValue as TryFrom<V>>::Error: Into<http::Error>,
    {
        self.config.header(key, value);
        self
    }

    /// Add a set of Headers to the existing ones on this download.
    /// The headers will be merged in to any already set.
    pub fn headers(mut self, headers: HeaderMap) -> Self {
        self.config.headers(headers);
        self
    }

    /// Override the the maxmimum number of times to consecutively retry the
    /// download without making any progress. Pass in `None` to retry forever.
    pub fn max_retries(mut self, max_retries: Option<u64>) -> Self {
        self.config.max_retries(max_retries);
        self
    }

    /// Set the progress reporter for this download.  The given reporter will
    /// be called periodically as data is downloaded.
    pub fn on_progress(
        mut self,
        progress: impl FnMut(&mut ProgressHandle) + Send + 'static,
    ) -> Self {
        self.config.on_progress(progress);
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
    /// # fn example() -> Result<(), Box<dyn std::error::Error>> {
    ///     let client = downlowd::blocking::Client::new();
    ///     let result = client.get("https://example.com/file.txt")
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
    ///        .send()?;
    /// #   Ok(())
    /// # }
    /// ```
    ///
    /// This can also be used to abort the download on a retry by calling `r.cancel()`:
    ///
    /// ```
    /// # use std::time::Duration;
    /// # fn example() -> Result<(), Box<dyn std::error::Error>> {
    ///     let client = downlowd::blocking::Client::new();
    ///     let result = client.get("http://localhost:8089/i_do_not_exist.txt")
    ///        .destination("file.txt")
    ///        .on_retry(|r| r.cancel())
    ///        .send();
    ///
    ///     assert!(matches!(result, Err(downlowd::Error::UnexpectedStatus { status: 404, .. })));
    /// #   Ok(())
    /// # }
    /// ```
    ///
    pub fn on_retry(mut self, retry: impl FnMut(&mut RetryHandle) + Send + 'static) -> Self {
        self.config.on_retry(retry);
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
        self.config.destination(destination);
        self
    }

    /// Set the etag for this file.  If you have already downloaded part of the
    /// file and know the etag, setting this will allow the download to verify
    /// that the file has not changed on the server before resuming.  If neither
    /// this nor the last modified time are set, then the mtime of the existing
    /// file on disk will be used in place of the last modified time.
    pub fn etag(mut self, etag: impl Into<String>) -> Self {
        self.config.etag(etag);
        self
    }

    /// Set the last modified time for this file.  If you have already downloaded
    /// part of the file and know the last modified time, setting this will allow
    /// the download to verify that the file has not changed on the server before
    /// resuming.  If neither this nor the etag are set, then the mtime of the existing
    /// file on disk will be used in place of the last modified time.
    pub fn last_modified(mut self, last_modified: impl Into<String>) -> Self {
        self.config.last_modified(last_modified);
        self
    }

    fn head(&mut self) -> &Head {
        if self.head.is_none() {
            let head = Head::create_blocking(&self.agent, &self.config).unwrap_or_default();
            self.head = Some(head);
        }
        self.head.as_ref().unwrap()
    }

    /// Returns the filename that downlowd will use when downloading the file.
    /// This is determined by making a HEAD request to the server, and looking
    /// at the `Content-Disposition` header, if present, or falling back to the
    /// last part of the URL path.
    pub fn get_remote_file_name(&mut self) -> &str {
        let head = self.head();
        head.get_remote_file_name()
    }

    /// Try to get the length of the remote file.  This may return None if the
    /// server doesn't provide a Content-Length header.
    fn get_remote_file_length(&mut self) -> Option<u64> {
        let head = self.head();
        head.get_remote_file_length()
    }

    /// Send the download request to the server.
    pub fn send(mut self) -> Result<DownloadResult, Error> {
        if let Some(e) = self.config.err {
            return Err(e);
        }

        // Work out where we're ultimately going to save the file.
        let destination = self.resolve_destination()?;

        // TODO: Allow forcing the download if the file already exists.
        if let Some(len) = self.recover(&destination) {
            // File already exists and is the correct length - nothing to do.
            return Ok(DownloadResult {
                tries: 0,
                path: destination.path,
                file_size: len,
                bytes_downloaded: 0,
            });
        }

        let inner = DownloadInner::new(self, destination)?;
        inner.download()
    }

    /// Recover from an existing file, if possible.  This handles the corner cases
    /// where we were close to being complete, but crashed or were cancelled
    /// right at the end.  If ths local file is the correct length and is complete,
    /// this returns the length of the file (indicating that the caller can skip
    /// downloading the file).
    fn recover(&mut self, destination: &Destination) -> Option<u64> {
        let local_part_length = fs::metadata(&destination.part_file)
            .ok()
            .map(|m| m.len());

        if local_part_length.is_some() {
            // We're partway through downloading the file.  Let the download continue.
            // If the part file is at 100% done, this will get handled below.
            return None;
        }

        let local_length = fs::metadata(&destination.path).ok().map(|m| m.len());

        if let Some(local_length) = local_length {
            let file_info = FileInfo::load_from_disk_blocking(&destination.sidecar_file).ok();
            let remote_length =
                if let Some(remote_length) = file_info.as_ref().and_then(|f| f.file_length) {
                    Some(remote_length)
                } else {
                    self.get_remote_file_length()
                };

            if let Some(remote_length) = remote_length
                && local_length == remote_length
            {
                // We have a copy of the file locally, which appears to be the correct length.
                if file_info.is_some() {
                    let _ = fs::remove_file(&destination.sidecar_file);
                }
                return Some(local_length);
            }
        }

        None
    }

    /// Returns the final destination path for the download.
    fn resolve_destination(&mut self) -> Result<Destination, Error> {
        let mut destination = self.config.configured_destination()?;

        // If the destination is a directory, figure out the filename for the file.
        let is_dir = std::fs::metadata(&destination)
            .map(|v| v.is_dir())
            .unwrap_or_default();
        if is_dir {
            let filename = self.get_remote_file_name();
            destination = destination.join(filename);
        };

        Ok(Destination::new(destination))
    }
}

impl DownloadInner {
    fn new(dl: Download, destination: Destination) -> Result<Self, Error> {
        // Use information provided by the user, or else load from the sidecar file if it exists.
        let mut local_file_info = dl.config.user_provided_local_file_info;
        if local_file_info.etag.is_none() && local_file_info.last_modified.is_none() {
            // User didn't tell us anything...
            let _ = local_file_info.load_blocking(&destination.sidecar_file);
        }

        // Open the part file for writing.
        let part_file = utils::file::open_file_for_writing(&destination.part_file)?;
        let file_length = part_file
            .metadata()
            .map_err(|e| Error::Write {
                action: "getting file metadata",
                path: destination.part_file.to_path_buf(),
                cause: e,
            })?
            .len();

        // This is the single instance of `ProgressHandle` that we'll update
        // and pass to the progress handler throughout the download.
        let progress = ProgressHandle::new(
            dl.config.url,
            dl.head.and_then(|h| h.updated_uri),
            destination,
            local_file_info,
            file_length,
        );

        Ok(Self {
            agent: dl.agent,
            limiter: dl.limiter,
            headers: dl.config.headers,
            max_retries: dl.config.max_retries,
            progress_handler: dl.config.progress_handler,
            retry_handler: dl.config.retry_handler,
            part_file,
            progress,
        })
    }

    fn download(mut self) -> Result<DownloadResult, Error> {
        let mut retries = 0;

        loop {
            if self.progress.is_complete() == Some(true) {
                // We already have the whole file!
                break;
            }

            self.progress.tries += 1;
            retries += 1;

            let bytes_before = self.progress.bytes_transferred;
            match self.try_download() {
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
                            utils::file::truncate_file(
                                &self.progress.destination.part_file,
                                &mut self.part_file,
                            )?;
                            self.progress.reset_blocking();
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

                        thread::sleep(retry_handle.delay);
                    }
                }
            }
        }

        // We're all done! Close the part_file.
        self.part_file.flush().ok();
        drop(self.part_file);

        // Rename the .part file to the final file.
        fs::rename(
            &self.progress.destination.part_file,
            &self.progress.destination.path,
        )
        .map_err(|e| Error::Write {
            action: "renaming part file",
            path: self.progress.destination.part_file.clone(),
            cause: e,
        })?;

        // Delete the sidecar file.
        let _ = fs::remove_file(&self.progress.destination.sidecar_file);

        Ok(DownloadResult::new(self.progress))
    }

    /// This is the "inner loop" of the download. Try to download the file, and return
    /// an error if it fails for any reason.  The caller can then decide whether to retry or not.
    fn try_download(&mut self) -> Result<(), Error> {
        // Make our GET request.
        let response = self.get_file()?;
        let status = response.status();

        if status == StatusCode::RANGE_NOT_SATISFIABLE {
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
        let append = status == StatusCode::PARTIAL_CONTENT;
        let remote_file_info =
            FileInfo::from_response(status, response.headers(), self.progress.bytes);

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
            .save_blocking(&self.progress.destination.sidecar_file);

        // Copy data from the response to the .part file.
        let result = self.copy_response_to_file(response, append);

        // Flush the file to ensure all data is written before we return.
        let _ = self.part_file.flush();
        result?;

        Ok(())
    }

    /// Send a GET request for the file.
    fn get_file(&mut self) -> Result<Response<Body>, Error> {
        let mut headers = self.headers.clone();
        add_resume_download_headers(&mut headers, &self.progress);
        let uri = self.progress.uri();
        let (u, response) = ureq_utils::ureq_request(&self.agent, http::Method::GET, uri, &headers);
        if u.is_some() {
            self.progress.updated_uri = u
        }

        if let Ok(response) = response.as_ref()
            && !response.status().is_success()
            && response.status() != http::StatusCode::RANGE_NOT_SATISFIABLE
        {
            return Err(Error::UnexpectedStatus {
                status: response.status().as_u16(),
            });
        }

        response
    }

    /// Stream data from the response to a file, and call into the progress callback as we go.
    /// Returns the total number of bytes written to the file, whether or not this succeeds.
    fn copy_response_to_file(
        &mut self,
        mut response: Response<Body>,
        append: bool,
    ) -> Result<u64, Error> {
        // The number of bytes downloaded on this attempt.
        let mut bytes_downloaded = 0;

        if !append {
            utils::file::truncate_file(&self.progress.destination.part_file, &mut self.part_file)?;
            self.progress.bytes = 0;
        }

        // Initial call into the progress callback.
        self.progress.notify(&mut self.progress_handler)?;

        let mut buf = [0; BUFFER_SIZE];
        let mut reader = response.body_mut().as_reader();

        loop {
            let n = reader.read(&mut buf).map_err(|cause| Error::Network {
                during: "read",
                uri: self.progress.uri().to_string(),
                cause: cause.to_string(),
            })?;
            if n == 0 {
                break;
            }
            let chunk = &buf[0..n];

            self.part_file
                .write_all(chunk)
                .map_err(|err| Error::Write {
                    action: "writing to file",
                    path: self.progress.destination.part_file.clone(),
                    cause: err,
                })?;

            let chunk_size = chunk.len() as u64;
            bytes_downloaded += chunk_size;
            self.progress
                .notify_bytes_written(&mut self.progress_handler, chunk_size)?;

            // Let the rate limiter know we downloaded some bytes.
            self.limiter.bytes_consumed(chunk.len() as u64);
            if !self.progress.is_complete().unwrap_or_default() {
                self.limiter.wait();
            }
        }

        Ok(bytes_downloaded)
    }
}
