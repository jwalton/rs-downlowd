use http::{HeaderMap, HeaderValue, header::IntoHeaderName};

use crate::{Download, Error, IntoUrl};

/// Builder for creating a `Client` with custom configuration.
pub struct ClientBuilder {
    user_agent: String,
    headers: HeaderMap,
    err: Option<Error>,
}

/// A client for downloading files over HTTP.
pub struct Client {
    client: reqwest::Client,
}

impl ClientBuilder {
    /// Create a new ClientBuilder with the given user agent.
    pub fn new() -> Self {
        ClientBuilder {
            user_agent: "downlow/1.0".to_string(),
            headers: HeaderMap::new(),
            err: None,
        }
    }

    /// Add a custom header to the client.
    pub fn header<K, V>(mut self, key: K, value: V) -> Self
    where
        K: IntoHeaderName,
        HeaderValue: TryFrom<V>,
        <HeaderValue as TryFrom<V>>::Error: Into<http::Error>,
    {
        match value.try_into() {
            Ok(v) => {
                self.headers.insert(key, v);
            }
            Err(e) => {
                self.err = Some(Error::InvalidHeader {
                    cause: e.into().to_string(),
                });
            }
        };

        self
    }

    /// Set default headers for the client.
    pub fn add_headers(mut self, headers: HeaderMap) -> Self {
        for (key, value) in headers.iter() {
            self.headers.insert(key, value.clone());
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
        Ok(Client { client })
    }
}

impl Client {
    /// Create a new client.
    pub fn new() -> Self {
        ClientBuilder::default().build().unwrap()
    }

    /// Create a file download.
    ///
    /// example:
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
        Download::create(self.client.clone(), url)
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
