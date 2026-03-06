use http::{
    HeaderMap, HeaderValue,
    header::{self, IntoHeaderName},
};

use crate::{
    DEFAULT_MAX_RETRIES, Error,
    utils::{
        self,
        http::{append_header, insert_header},
    },
};

/// Builder for creating a `Client` with custom configuration.
pub struct ClientBuilder {
    pub(crate) headers: HeaderMap,
    pub(crate) default_max_retries: Option<u64>,
    pub(crate) max_bytes_per_second: Option<u64>,
    pub(crate) err: Option<Error>,
    #[cfg(feature = "async")]
    pub(crate) reqwest_client: Option<reqwest::Client>,
}

impl ClientBuilder {
    /// Create a new ClientBuilder with the given user agent.
    pub fn new() -> Self {
        ClientBuilder {
            headers: HeaderMap::new(),
            default_max_retries: DEFAULT_MAX_RETRIES,
            max_bytes_per_second: None,
            err: None,
            #[cfg(feature = "async")]
            reqwest_client: None,
        }
    }

    /// Set the user agent for the client.
    pub fn user_agent<V>(mut self, user_agent: V) -> Self
    where
        HeaderValue: TryFrom<V>,
        <HeaderValue as TryFrom<V>>::Error: Into<http::Error>,
    {
        if let Err(e) = insert_header(&mut self.headers, header::USER_AGENT, user_agent) {
            self.err = Some(e);
        }
        self
    }

    /// Add a default header for every request.  If the key has already been
    /// added as a header, then the value will be pushed to the end of the list
    /// of values associated witht his header (like [HeaderMap::append]).
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

    /// Set the reqwest client instance to use when making nonblocking requests.
    /// This only applies to nonblocking clients, but will allow you to specify
    /// a reqwest client with advanced options configured, or allow you to share
    /// a connection pool across multiple downlowd clients.
    #[cfg(feature = "async")]
    pub fn reqwest_client(mut self, reqwest_client: reqwest::Client) -> Self {
        self.reqwest_client = Some(reqwest_client);
        self
    }
}

impl Default for ClientBuilder {
    fn default() -> Self {
        Self::new()
    }
}
