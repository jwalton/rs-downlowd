use std::{path::PathBuf, sync::Arc, time::Duration};

use http::{HeaderMap, Uri};

use crate::{
    DEFAULT_MAX_DELAY, DEFAULT_MIN_DELAY, DownloadResult, Error, Progress, ProgressHandle,
    RetryHandle, RetryHandler,
    destination::Destination,
    file_info::FileInfo,
    headers,
    maybe_async::{Client, File, Limiter, Response, System},
    shared::{DownloadConfig, LazyHead},
};

pub struct DownloadInner<C, F, L> {
    /// The client to use to download the file.
    client: C,
    /// Rate limiter.
    limiter: Arc<L>,
    /// Headers to include in the request.
    headers: HeaderMap,
    /// The maximum number of times we can consecutively retry without making any progress.
    max_retries: Option<u64>,
    /// Progress callback, if any.
    progress_handler: Option<Box<dyn Progress + Send>>,
    /// The handler to call when we retry a download.
    retry_handler: RetryHandler,
    /// File we're writing to.
    part_file: F,
    /// Information about the remote file, if we need to retrieve it.
    head: LazyHead,
    /// Progress handle keeps track of information about the download, and is
    /// send to the progress callback.
    progress: ProgressHandle,
}

impl<C, F, L> DownloadInner<C, F, L>
where
    C: Client,
    F: File,
    L: Limiter,
{
    pub async fn new(
        client: C,
        limiter: Arc<L>,
        config: DownloadConfig,
        destination: PathBuf,
        mut head: LazyHead,
    ) -> Result<Self, Error> {
        let destination = Self::resolve_destination(
            &mut head,
            &client,
            &config.uri,
            &config.headers,
            destination,
        )
        .await?;

        // Open the part file for writing.  We do this first thing, because this also
        // takes a lock for the file, so now we know we're in control of this file
        // and no other processes should be reading/writing to it.
        let part_file = F::open_for_writing(&destination.part_file).await?;
        let file_length = part_file.get_length().await?;

        // Use information provided by the user, or else load from the sidecar file if it exists.
        let mut local_file_info = config.user_provided_local_file_info;
        if local_file_info.etag.is_none() && local_file_info.last_modified.is_none() {
            // User didn't tell us anything...
            let _ = local_file_info.load::<F>(&destination.sidecar_file).await;
        }

        // This is the single instance of `ProgressHandle` that we'll update
        // and pass to the progress handler throughout the download.
        let progress = ProgressHandle::new(
            config.uri,
            head.try_get().and_then(|h| h.updated_uri.clone()),
            destination,
            local_file_info,
            file_length,
        );

        Ok(Self {
            client,
            limiter,
            headers: config.headers,
            max_retries: config.max_retries,
            progress_handler: config.progress_handler,
            retry_handler: config.retry_handler,
            part_file,
            head,
            progress,
        })
    }

    /// Returns the final destination path for the download.
    async fn resolve_destination(
        head: &mut LazyHead,
        client: &C,
        uri: &Uri,
        headers: &HeaderMap,
        mut destination: PathBuf,
    ) -> Result<Destination, Error> {
        // If the destination is a directory, figure out the filename for the file.
        let is_dir = F::metadata(&destination)
            .await
            .map(|m| m.is_dir())
            .unwrap_or_default();
        if is_dir {
            let filename = head.get(client, uri, headers).await.get_remote_file_name();
            destination = destination.join(filename);
        };

        Ok(Destination::new(destination))
    }

    pub async fn download<S: System>(mut self) -> Result<DownloadResult, Error> {
        let mut retries = 0;

        if self.recover().await {
            // All done!
            let _ = F::remove_file(&self.progress.destination.part_file).await;
        } else {
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
                                self.part_file.truncate().await?;
                                self.progress.bytes = 0;
                                // Reset the local file info. It'll get filled in again
                                // at the start of the next download attempt.
                                self.progress
                                    .local_file_info
                                    .reset::<F>(&self.progress.destination.sidecar_file)
                                    .await;
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

                            S::sleep(retry_handle.delay).await;
                        }
                    }
                }
            }

            // Rename the .part file to the final file.
            self.part_file
                .rename(&self.progress.destination.path)
                .await?;
        }

        // Close the part_file.
        drop(self.part_file);

        // Delete the sidecar file.
        let _ = F::remove_file(&self.progress.destination.sidecar_file).await;

        Ok(DownloadResult::new(self.progress))
    }

    /// Recover from an existing file, if possible.  This handles the corner cases
    /// where we were close to being complete, but crashed or were cancelled
    /// right at the end.  If ths local file is the correct length and is complete,
    /// this returns true (indicating that the caller can skip
    /// downloading the file).
    async fn recover(&mut self) -> bool {
        // See if the "final" file exists.
        let local_length = F::metadata(&self.progress.destination.path)
            .await
            .map(|m| m.len())
            .ok();

        if let Some(local_length) = local_length {
            let remote_length = match self.progress.remote_length() {
                Some(remote_length) => Some(remote_length),
                None => self
                    .head
                    .get(&self.client, self.progress.uri(), &self.headers)
                    .await
                    .get_remote_file_length(),
            };

            if remote_length == Some(local_length) {
                // Seems like we have the whole file.  We can just delete the part
                // file and sidecar file.
                return true;
            }
        }

        false
    }

    /// This is the "inner loop" of the download. Try to download the file, and return
    /// an error if it fails for any reason.  The caller can then decide whether to retry or not.
    async fn try_download(&mut self) -> Result<(), Error> {
        // Make our GET request.
        let response = self.get_file().await?;
        let status = response.status();

        if status == http::StatusCode::RANGE_NOT_SATISFIABLE {
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
        let append = status == http::StatusCode::PARTIAL_CONTENT;
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
            .save::<F>(&self.progress.destination.sidecar_file)
            .await;

        // Copy data from the response to the .part file.
        let result = self.copy_response_to_file(response, append).await;

        // Flush the file to ensure all data is written before we return.
        let _ = self.part_file.sync_all().await;
        result?;

        Ok(())
    }

    /// Send a GET request for the file.
    async fn get_file(&mut self) -> Result<C::Response, Error> {
        let mut headers = self.headers.clone();
        headers::add_resume_download_headers(&mut headers, &self.progress);
        let uri = self.progress.uri();
        let (u, response) = self.client.request(http::Method::GET, uri, headers).await;
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
    async fn copy_response_to_file(
        &mut self,
        mut response: C::Response,
        append: bool,
    ) -> Result<u64, Error> {
        // The number of bytes downloaded on this attempt.
        let mut bytes_downloaded = 0;

        if !append {
            self.part_file.truncate().await?;
            self.progress.bytes = 0;
        }

        // Initial call into the progress callback.
        self.progress.notify(&mut self.progress_handler)?;

        while let Some(chunk) =
            response
                .chunk(self.progress.uri())
                .await
                .map_err(|cause| Error::Network {
                    uri: self.progress.uri().to_string(),
                    cause: cause.to_string(),
                })?
        {
            let chunk_size = chunk.len() as u64;
            self.part_file.write_all(&chunk).await?;

            bytes_downloaded += chunk_size;
            self.progress
                .notify_bytes_written(&mut self.progress_handler, chunk_size)?;

            // Let the rate limiter know we downloaded some bytes.
            self.limiter.bytes_consumed(chunk_size).await;
            if !self.progress.is_complete().unwrap_or_default() {
                self.limiter.wait().await;
            }
        }

        Ok(bytes_downloaded)
    }
}
