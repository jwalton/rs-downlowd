use std::sync::Arc;

use http::{HeaderMap, HeaderValue, header::IntoHeaderName};

use crate::{
    DEFAULT_MAX_RETRIES, Download, Error, IntoUrl,
    limiter::TokioLimiter,
    utils::{self, http::append_header},
};

/// Builder for creating a `Client` with custom configuration.
pub struct ClientBuilder {
    user_agent: Option<String>,
    headers: HeaderMap,
    default_max_retries: Option<u64>,
    max_bytes_per_second: Option<u64>,
    err: Option<Error>,
}

/// A client for downloading files over HTTP.  A `Client` uses an internal
/// connection pool to manage HTTP connections, and has a shared rate limiter,
/// so it is recommended to create a single `Client` and reuse it for multiple
/// downloads.  Clients are cheap to clone.
#[derive(Clone)]
pub struct Client {
    client: reqwest::Client,
    default_max_retries: Option<u64>,
    limiter: Arc<TokioLimiter>,
}

impl ClientBuilder {
    /// Create a new ClientBuilder with the given user agent.
    pub fn new() -> Self {
        ClientBuilder {
            user_agent: None,
            headers: HeaderMap::new(),
            default_max_retries: DEFAULT_MAX_RETRIES,
            max_bytes_per_second: None,
            err: None,
        }
    }

    /// Set the user agent for the client.
    pub fn user_agent(mut self, user_agent: impl Into<String>) -> Self {
        self.user_agent = Some(user_agent.into());
        self
    }

    /// Add a default header for every request.
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

    /// Set the default headers for every request.
    pub fn headers(mut self, headers: HeaderMap) -> Self {
        utils::http::append_all_headers(&mut self.headers, headers);
        self
    }

    /// Set the default maxmimum number of times to consecutively retry a download
    /// without making any progress. The default is 5. This counter resets whenever
    /// at least one byte of data is downloaded from the server. Pass in `None`
    /// to retry forever.
    pub fn max_retries(mut self, max_retries: Option<u64>) -> Self {
        self.default_max_retries = max_retries;
        self
    }

    /// Set the maximum bytes per second that can be downloaded. This limit is
    /// shared across all downloads using this client.
    pub fn max_bytes_per_second(mut self, max: Option<u64>) -> Self {
        if max == Some(0) {
            self.err = Some(Error::InvalidConfig {
                message: "max_bytes_per_second must be greater than 0".to_string(),
            });
        } else {
            self.max_bytes_per_second = max;
        }
        self
    }

    /// Build the client.
    pub fn build(self) -> Result<Client, Error> {
        if let Some(e) = self.err {
            return Err(e);
        }

        let mut builder = reqwest::Client::builder();
        if let Some(user_agent) = &self.user_agent {
            builder = builder.user_agent(user_agent);
        }
        let client = builder
            .default_headers(self.headers)
            .build()
            .expect("Failed to create HTTP client");

        let limiter = Arc::new(TokioLimiter::new(self.max_bytes_per_second));

        Ok(Client {
            client,
            default_max_retries: self.default_max_retries,
            limiter,
        })
    }
}

impl Client {
    /// Create a new client.
    pub fn new() -> Self {
        ClientBuilder::default().build().unwrap()
    }

    /// Build a new client.
    pub fn builder() -> ClientBuilder {
        ClientBuilder::new()
    }

    /// Create a file download.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    ///     let client = downlowd::Client::new();
    ///     let result = client.download("https://example.com/file.txt")
    ///        .destination("file.txt")
    ///        .send()
    ///        .await?;
    /// #   Ok(())
    /// # }
    /// ```
    ///
    pub fn download(&self, url: impl IntoUrl) -> Download {
        Download::create(
            self.client.clone(),
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

impl Default for ClientBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl Default for Client {
    fn default() -> Self {
        Self::new()
    }
}
