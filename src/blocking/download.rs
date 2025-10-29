use std::{path::Path, sync::Arc};

use http::{HeaderMap, HeaderValue, header::IntoHeaderName};

use crate::{
    DownloadResult, Error, ProgressHandle, RetryHandle,
    feat::{
        blocking_token_bucket::BlockingTokenBucket, std_file::StdFile, std_system::StdSystem,
        ureq_client::UreqClient,
    },
    head::Head,
    shared::{self, DownloadConfig, DownloadInner},
};

/// Represents a file about to be downloaded.
pub struct Download {
    /// The client to use to download the file.
    client: UreqClient,
    /// Rate limiter.
    limiter: Arc<BlockingTokenBucket>,
    /// How do we want to download this file?
    config: DownloadConfig,
    /// Information about the remote file, if we need to retrieve it.
    head: Option<Head>,
}

impl Download {
    /// Create a new download for the given URL.
    pub(crate) fn new(
        client: UreqClient,
        limiter: Arc<BlockingTokenBucket>,
        config: DownloadConfig,
    ) -> Self {
        Download {
            client,
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

    /// Returns the filename that downlowd will use when downloading the file.
    /// This is determined by making a HEAD request to the server, and looking
    /// at the `Content-Disposition` header, if present, or falling back to the
    /// last part of the URL path.
    pub fn get_remote_file_name(&mut self) -> &str {
        sync_executor::block_on(shared::get_remote_file_name(
            &mut self.head,
            &self.client,
            &self.config.uri,
            &self.config.headers,
        ))
        .unwrap()
    }

    /// Send the download request to the server.
    pub fn send(self) -> Result<DownloadResult, Error> {
        if let Some(e) = self.config.err {
            return Err(e);
        }

        let destination = self.config.configured_destination()?;

        sync_executor::block_on(async {
            let inner: DownloadInner<_, StdFile, _> = DownloadInner::new(
                self.client,
                self.limiter,
                self.config,
                destination,
                self.head,
            )
            .await?;
            inner.download::<StdSystem>().await
        })
        .unwrap()
    }
}
