mod client;
mod download;
mod reqwest_utils;
mod tokio_tokenbucket;

#[cfg(test)]
mod tests;

pub use client::Client;
pub use download::Download;
use http::HeaderMap;
use url::Url;

use crate::{Error, head::Head};

impl Head {
    /// Send a HEAD request to the server to get information about a URL.
    pub async fn create(
        client: &reqwest::Client,
        url: &Url,
        headers: &HeaderMap,
    ) -> Result<Self, Error> {
        // TODO: Retry the HEAD request if it fails with a retryable error.
        let (updated_url, head) = reqwest_utils::request(
            client,
            reqwest::Method::HEAD,
            url.clone(),
            headers.clone(),
        )
        .await;

        let response = head?;
        Self::create_inner(response.status(), response.headers(), url, updated_url)
    }
}
