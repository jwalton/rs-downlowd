use std::sync::Arc;

use http::HeaderMap;

use crate::{
    ClientBuilder, Error, IntoUri,
    nonblocking::{
        download::Download,
        imp::{reqwest_client::ReqwestClient, tokio_token_bucket::TokioTokenBucket},
    },
    shared::DownloadConfig,
};

/// A client for downloading files over HTTP.  A `Client` uses an internal
/// connection pool to manage HTTP connections, and has a shared rate limiter,
/// so it is recommended to create a single `Client` and reuse it for multiple
/// downloads.  Clients are cheap to clone.
#[derive(Clone)]
pub struct Client {
    client: ReqwestClient,
    headers: HeaderMap,
    default_max_retries: Option<u64>,
    limiter: Arc<TokioTokenBucket>,
}

impl ClientBuilder {
    /// Build the client.
    pub fn build(self) -> Result<Client, Error> {
        if let Some(e) = self.err {
            return Err(e);
        }

        let client = self.reqwest_client.unwrap_or_default();

        Ok(Client::new_inner(
            ReqwestClient::new(client),
            self.headers,
            self.default_max_retries,
            self.max_bytes_per_second,
        ))
    }
}

impl Client {
    /// Create a new client.
    pub fn new() -> Self {
        ClientBuilder::default().build().unwrap()
    }

    fn new_inner(
        client: ReqwestClient,
        headers: HeaderMap,
        default_max_retries: Option<u64>,
        max_bytes_per_second: Option<u64>,
    ) -> Self {
        let limiter = Arc::new(TokioTokenBucket::new(max_bytes_per_second));

        Self {
            client,
            headers,
            default_max_retries,
            limiter,
        }
    }

    /// Build a new client.
    pub fn builder() -> ClientBuilder {
        ClientBuilder::new()
    }

    /// Used to download a file.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    ///     let client = downlowd::Client::new();
    ///     let result = client.get("https://example.com/file.txt")
    ///        .destination("file.txt")
    ///        .send()
    ///        .await?;
    /// #   Ok(())
    /// # }
    /// ```
    ///
    pub fn get(&self, url: impl IntoUri) -> Download {
        let mut config = DownloadConfig::new(url, &self.headers);
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
