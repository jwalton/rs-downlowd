mod client_builder;
mod client;
mod download;
mod ureq_utils;
mod tokenbucket;

use crate::{Error, shared::DownloadConfig, head::Head};

pub use client_builder::BlockingClientBuilder;
pub use client::Client;
pub use download::Download;

impl Head {
    /// Send a HEAD request to the server to get information about a URL.
    pub fn create_blocking(agent: &ureq::Agent, config: &DownloadConfig) -> Result<Self, Error> {
        // TODO: Retry the HEAD request if it fails with a retryable error.
        let (updated_url, head) = crate::blocking::ureq_utils::ureq_request(
            agent,
            http::Method::HEAD,
            &config.url,
            &config.headers,
        );

        let response = head?;
        Self::create_inner(
            response.status(),
            response.headers(),
            &config.url,
            updated_url,
        )
    }
}
