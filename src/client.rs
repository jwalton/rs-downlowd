use std::sync::Arc;

use http::{HeaderMap, HeaderValue, header::IntoHeaderName};

use crate::{Download, Error, IntoUrl, limiter::TokioLimiter, utils};

/// Builder for creating a `Client` with custom configuration.
pub struct ClientBuilder {
    user_agent: String,
    headers: HeaderMap,
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
    limiter: Arc<TokioLimiter>,
}

impl ClientBuilder {
    /// Create a new ClientBuilder with the given user agent.
    pub fn new() -> Self {
        ClientBuilder {
            user_agent: "downlow/1.0".to_string(),
            headers: HeaderMap::new(),
            max_bytes_per_second: None,
            err: None,
        }
    }

    /// Set the user agent for the client.
    pub fn user_agent(mut self, user_agent: impl Into<String>) -> Self {
        self.user_agent = user_agent.into();
        self
    }

    /// Add a default header for every request.
    pub fn header<K, V>(mut self, key: K, value: V) -> Self
    where
        K: IntoHeaderName,
        HeaderValue: TryFrom<V>,
        <HeaderValue as TryFrom<V>>::Error: Into<http::Error>,
    {
        match value.try_into() {
            Ok(v) => {
                self.headers.append(key, v);
            }
            Err(e) => {
                self.err = Some(Error::InvalidHeader {
                    cause: e.into().to_string(),
                });
            }
        };

        self
    }

    /// Set the default headers for every request.
    pub fn headers(mut self, headers: HeaderMap) -> Self {
        utils::http::append_all_headers(&mut self.headers, headers);
        self
    }

    /// Set the maximum bytes per second that can be downloaded. This limit is
    /// shared across all downloads using this client.
    pub fn max_bytes_per_second(mut self, max: u64) -> Self {
        if max == 0 {
            self.err = Some(Error::InvalidConfig {
                message: "max_bytes_per_second must be greater than 0".to_string(),
            });
        } else {
            self.max_bytes_per_second = Some(max);
        }
        self
    }

    /// Build the client.
    pub fn build(self) -> Result<Client, Error> {
        if let Some(e) = self.err {
            return Err(e);
        }

        let client = reqwest::Client::builder()
            .user_agent(&self.user_agent)
            .default_headers(self.headers)
            .build()
            .expect("Failed to create HTTP client");

        let limiter = Arc::new(TokioLimiter::new(self.max_bytes_per_second));

        Ok(Client { client, limiter })
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
    ///     let client = downlow::Client::new();
    ///     let result = client.download("https://example.com/file.txt")
    ///        .destination("file.txt")
    ///        .download()
    ///        .await?;
    /// #   Ok(())
    /// # }
    /// ```
    ///
    pub fn download(&self, url: impl IntoUrl) -> Download {
        Download::create(self.client.clone(), self.limiter.clone(), url)
    }

    /// Update the maximum bytes per second that can be downloaded. This limit
    /// is shared across all downloads using this client. Setting this to `None`
    /// removes any rate limit.
    pub async fn max_bytes_per_second(&self, max_bytes_per_second: Option<u64>) {
        self.limiter.set_max_bytes_per_second(max_bytes_per_second).await;
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
