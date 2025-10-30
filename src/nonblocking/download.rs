use std::sync::Arc;

use crate::{
    DownloadResult, Error, RetryHandle,
    nonblocking::imp::{
        reqwest_client::ReqwestClient, tokio_file::TokioFile, tokio_system::TokioSystem,
        tokio_token_bucket::TokioTokenBucket,
    },
    shared::{DownloadConfig, DownloadInner, LazyHead, config_proxy},
};

/// Represents a file about to be downloaded.
pub struct Download {
    /// The client to use to download the file.
    client: ReqwestClient,
    /// Rate limiter.
    limiter: Arc<TokioTokenBucket>,
    /// How do we want to download this file?
    config: DownloadConfig,
    /// Information about the remote file, if we need to retrieve it.
    head: LazyHead,
}

impl Download {
    /// Create a new download for the given URL.
    pub(crate) fn new(
        client: ReqwestClient,
        limiter: Arc<TokioTokenBucket>,
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
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    ///     let client = downlowd::Client::new();
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
    ///     let result = client.get("http://localhost:8089/i_do_not_exist.txt")
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
        self.config.on_retry(retry);
        self
    }

    /// Returns the filename that downlowd will use when downloading the file.
    /// This is determined by making a HEAD request to the server, and looking
    /// at the `Content-Disposition` header, if present, or falling back to the
    /// last part of the URL path.
    pub async fn get_remote_file_name(&mut self) -> &str {
        // FIXME: If someone calls this, and then changes the headers or other parameters of this call,
        // if might change the result of the `HEAD` request, which might invalidate our LazyHead.
        self.head
            .get(&self.client, &self.config.uri, &self.config.headers)
            .await
            .get_remote_file_name()
    }

    /// Send the download request to the server.
    pub async fn send(self) -> Result<DownloadResult, Error> {
        if let Some(e) = self.config.err {
            return Err(e);
        }

        // Work out where we're ultimately going to save the file.
        let destination = self.config.configured_destination()?;

        let inner: DownloadInner<_, TokioFile, _> = DownloadInner::new(
            self.client,
            self.limiter,
            self.config,
            destination,
            self.head,
        )
        .await?;
        inner.download::<TokioSystem>().await
    }
}
