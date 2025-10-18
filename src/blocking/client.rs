use std::sync::Arc;

use crate::{
    IntoUrl,
    blocking::{BlockingClientBuilder, download::Download, tokenbucket::BlockingTokenBucket},
    client_builder::ClientBuilder,
};

/// A client for downloading files over HTTP.  A `Client` uses an internal
/// connection pool to manage HTTP connections, and has a shared rate limiter,
/// so it is recommended to create a single `Client` and reuse it for multiple
/// downloads.  Clients are cheap to clone.
#[derive(Clone)]
pub struct Client {
    agent: ureq::Agent,
    default_max_retries: Option<u64>,
    limiter: Arc<BlockingTokenBucket>,
}

impl Client {
    /// Create a new client.
    pub fn new() -> Self {
        ClientBuilder::default().blocking().unwrap()
    }

    pub(crate) fn new_inner(
        agent: ureq::Agent,
        default_max_retries: Option<u64>,
        max_bytes_per_second: Option<u64>,
    ) -> Self {
        let limiter = Arc::new(BlockingTokenBucket::new(max_bytes_per_second));

        Self {
            agent,
            default_max_retries,
            limiter,
        }
    }

    /// Build a new client.
    pub fn builder() -> BlockingClientBuilder {
        BlockingClientBuilder::default()
    }

    /// Create a file download.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    ///     let client = downlowd::blocking::Client::new();
    ///     let result = client.download("https://example.com/file.txt")
    ///        .destination("file.txt")
    ///        .send()?;
    /// #   Ok(())
    /// # }
    /// ```
    ///
    pub fn download(&self, url: impl IntoUrl) -> Download {
        Download::new(
            self.agent.clone(),
            self.default_max_retries,
            self.limiter.clone(),
            url,
        )
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
