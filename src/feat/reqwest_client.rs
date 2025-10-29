use ::http::{HeaderMap, Method, Uri};
use bytes::Bytes;

use crate::{Error, head::Head, maybe_async};

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

    async fn chunk(&mut self, uri: &Uri) -> Result<Option<Bytes>, Error> {
        self.inner.chunk().await.map_err(|cause| Error::Network {
            during: "read",
            uri: uri.to_string(),
            cause: cause.without_url().to_string(),
        })
    }
}

impl ReqwestClient {
    pub fn new(client: reqwest::Client) -> Self {
        Self { client }
    }
}

impl maybe_async::Client for ReqwestClient {
    type Response = Response;

    async fn head(&self, uri: &Uri, headers: &HeaderMap) -> Head {
        // TODO: Retry the HEAD request if it fails with a retryable error.
        let (updated_uri, head) = self.request(Method::HEAD, uri, headers.clone()).await;

        if let Ok(response) = head {
            Head::create_inner(
                response.inner.status(),
                response.inner.headers(),
                uri,
                updated_uri,
            )
            .unwrap_or_default()
        } else {
            Head::default()
        }
    }

    async fn request(
        &self,
        method: Method,
        uri: &Uri,
        headers: HeaderMap,
    ) -> (Option<Uri>, Result<Self::Response, crate::Error>) {
        let url = uri.to_string();
        let method_name = match method {
            reqwest::Method::GET => "GET",
            reqwest::Method::HEAD => "HEAD",
            _ => "REQUEST",
        };

        // Reqwest follows redirect automatically.
        let response = self
            .client
            .request(method, &url)
            .headers(headers)
            .send()
            .await
            .map_err(|cause| {
                if cause.is_redirect() {
                    Error::BadRedirect {
                        reason: "too many redirects",
                    }
                } else {
                    Error::Network {
                        during: method_name,
                        uri: url.clone(),
                        cause: cause.without_url().to_string(),
                    }
                }
            });

        let response = match response {
            Ok(r) => r,
            Err(e) => {
                return (None, Err(e));
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
