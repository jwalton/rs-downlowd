use http::{HeaderMap, HeaderValue, header::IntoHeaderName};

use crate::{Error, blocking::Client, client_builder::ClientBuilder};

/// Builder for blocking clients.
#[derive(Default)]
pub struct BlockingClientBuilder {
    builder: ClientBuilder,
}

impl ClientBuilder {
    /// Build a blocking client.
    pub(crate) fn blocking(self) -> Result<Client, Error> {
        if let Some(e) = self.err {
            return Err(e);
        }

        let config = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .build();

        Ok(Client::new_inner(
            config.into(),
            self.headers,
            self.default_max_retries,
            self.max_bytes_per_second,
        ))
    }
}

impl BlockingClientBuilder {
    /// Create a new ClientBuilder with the given user agent.
    pub fn new() -> Self {
        Self {
            builder: ClientBuilder::default(),
        }
    }

    /// Set the user agent for the client.
    pub fn user_agent<V>(mut self, user_agent: V) -> Self
    where
        HeaderValue: TryFrom<V>,
        <HeaderValue as TryFrom<V>>::Error: Into<http::Error>,
    {
        self.builder = self.builder.user_agent(user_agent);
        self
    }

    /// Add a default header for every request.
    pub fn header<K, V>(mut self, key: K, value: V) -> Self
    where
        K: IntoHeaderName,
        HeaderValue: TryFrom<V>,
        <HeaderValue as TryFrom<V>>::Error: Into<http::Error>,
    {
        self.builder = self.builder.header(key, value);
        self
    }

    /// Set the default headers for every request.
    pub fn headers(mut self, headers: HeaderMap) -> Self {
        self.builder = self.builder.headers(headers);
        self
    }

    /// Set the default maxmimum number of times to consecutively retry a download
    /// without making any progress. The default is 5. This counter resets whenever
    /// at least one byte of data is downloaded from the server. Pass in `None`
    /// to retry forever.
    pub fn max_retries(mut self, max_retries: Option<u64>) -> Self {
        self.builder = self.builder.max_retries(max_retries);
        self
    }

    /// Set the maximum bytes per second that can be downloaded. This limit is
    /// shared across all downloads using this client.
    pub fn max_bytes_per_second(mut self, max: Option<u64>) -> Self {
        self.builder = self.builder.max_bytes_per_second(max);
        self
    }

    /// Build a blocking client.
    pub fn build(self) -> Result<Client, Error> {
        self.builder.blocking()
    }
}

impl From<ClientBuilder> for BlockingClientBuilder {
    fn from(builder: ClientBuilder) -> Self {
        Self { builder }
    }
}
