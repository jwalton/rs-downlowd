use std::sync::Arc;

use crate::{
    DownloadResult, Error, RetryHandle,
    blocking::imp::{
        blocking_token_bucket::BlockingTokenBucket, std_file::StdFile, std_system::StdSystem,
        ureq_client::UreqClient,
    },
    shared::{DownloadConfig, DownloadInner, LazyHead, config_proxy},
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
    head: LazyHead,
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
            head: LazyHead::default(),
        }
    }

    // Expose functions to update self.config.
    config_proxy! {}

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

    /// Returns the filename that downlowd will use when downloading the file.
    /// This is determined by making a HEAD request to the server, and looking
    /// at the `Content-Disposition` header, if present, or falling back to the
    /// last part of the URL path.
    pub fn get_remote_file_name(&mut self) -> &str {
        sync_executor::block_on(
            self.head
                .get(&self.client, &self.config.uri, &self.config.headers),
        )
        .unwrap()
        .get_remote_file_name()
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
