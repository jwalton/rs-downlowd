use std::sync::Arc;

use http::HeaderMap;

use crate::{
    IntoUri,
    blocking::{
        BlockingClientBuilder,
        download::Download,
        imp::{blocking_token_bucket::BlockingTokenBucket, ureq_client::UreqClient},
    },
    shared::DownloadConfig,
};

/// A client for downloading files over HTTP.  A `Client` uses an internal
/// connection pool to manage HTTP connections, and has a shared rate limiter,
/// so it is recommended to create a single `Client` and reuse it for multiple
/// downloads.  Clients are cheap to clone.
#[derive(Clone)]
pub struct Client {
    client: UreqClient,
    headers: HeaderMap,
    default_max_retries: Option<u64>,
    limiter: Arc<BlockingTokenBucket>,
}

impl Client {
    /// Create a new client.
    pub fn new() -> Self {
        BlockingClientBuilder::default().build().unwrap()
    }

    pub(crate) fn new_inner(
        agent: ureq::Agent,
        headers: HeaderMap,
        default_max_retries: Option<u64>,
        max_bytes_per_second: Option<u64>,
    ) -> Self {
        let limiter = Arc::new(BlockingTokenBucket::new(max_bytes_per_second));

        Self {
            client: UreqClient::new(agent),
            headers,
            default_max_retries,
            limiter,
        }
    }

    /// Build a new client.
    pub fn builder() -> BlockingClientBuilder {
        BlockingClientBuilder::default()
    }

    /// Used to download a file.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    ///     let client = downlowd::blocking::Client::new();
    ///     let result = client.get("https://example.com/file.txt")
    ///        .destination("file.txt")
    ///        .send()?;
    /// #   Ok(())
    /// # }
    /// ```
    ///
    pub fn get(&self, uri: impl IntoUri) -> Download {
        let mut config = DownloadConfig::new(uri, &self.headers);
        config.max_retries(self.default_max_retries);

        Download::new(self.client.clone(), self.limiter.clone(), config)
    }

    /// Update the maximum bytes per second that can be downloaded. This limit
    /// is shared across all downloads using this client. Setting this to `None`
    /// removes any rate limit.
    pub fn max_bytes_per_second(&self, max_bytes_per_second: Option<u64>) {
        self.limiter.set_max_bytes_per_second(max_bytes_per_second);
    }
}

impl Default for Client {
    fn default() -> Self {
        Self::new()
    }
}
