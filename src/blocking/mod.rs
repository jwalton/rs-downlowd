mod client;
mod client_builder;
mod download;
mod tokenbucket;
mod ureq_utils;

use crate::{Error, head::Head};

pub use client::Client;
pub use client_builder::BlockingClientBuilder;
pub use download::Download;
use http::{HeaderMap, Uri};

impl Head {
    /// Send a HEAD request to the server to get information about a URL.
    pub fn create_blocking(
        agent: &ureq::Agent,
        uri: &Uri,
        headers: &HeaderMap,
    ) -> Result<Self, Error> {
        // TODO: Retry the HEAD request if it fails with a retryable error.
        let (updated_uri, head) =
            crate::blocking::ureq_utils::ureq_request(agent, http::Method::HEAD, uri, headers);

        let response = head?;
        Self::create_inner(response.status(), response.headers(), uri, updated_uri)
    }
}
