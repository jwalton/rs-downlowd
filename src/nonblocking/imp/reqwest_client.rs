use ::http::{HeaderMap, Method, Uri};
use bytes::Bytes;

use crate::{Error, maybe_async};

#[derive(Clone)]
pub struct ReqwestClient {
    pub client: reqwest::Client,
}

pub struct Response {
    inner: reqwest::Response,
}

impl maybe_async::Response for Response {
    fn status(&self) -> http::StatusCode {
        self.inner.status()
    }

    fn headers(&self) -> &HeaderMap {
        self.inner.headers()
    }

    async fn chunk(&mut self, _uri: &Uri) -> Result<Option<Bytes>, Error> {
        Ok(self.inner.chunk().await?)
    }
}

impl ReqwestClient {
    pub fn new(client: reqwest::Client) -> Self {
        Self { client }
    }
}

impl maybe_async::Client for ReqwestClient {
    type Response = Response;

    async fn request(
        &self,
        method: Method,
        uri: &Uri,
        headers: HeaderMap,
    ) -> (Option<Uri>, Result<Self::Response, crate::Error>) {
        let url = uri.to_string();

        // Reqwest follows redirect automatically.
        let response = self
            .client
            .request(method, &url)
            .headers(headers)
            .send()
            .await;

        let response = match response {
            Ok(r) => r,
            Err(e) => {
                return (None, Err(e.into()));
            }
        };

        let response_url = response.url().as_str();
        let returned_uri = if response_url != url {
            response_url.parse::<Uri>().ok()
        } else {
            None
        };

        (returned_uri, Ok(Response { inner: response }))
    }
}

impl From<reqwest::Error> for Error {
    fn from(value: reqwest::Error) -> Self {
        if value.is_redirect() {
            Error::BadRedirect {
                reason: "too many redirects",
            }
        } else {
            Error::Network {
                uri: value.url().map(|u| u.to_string()).unwrap_or_default(),
                cause: value.without_url().to_string(),
            }
        }
    }
}
