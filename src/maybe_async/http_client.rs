use bytes::Bytes;
use http::{HeaderMap, Method, StatusCode, Uri};

use crate::{Error, head::Head};

pub trait Client {
    type Response: Response;

    async fn head(&self, uri: &Uri, headers: &HeaderMap) -> Head {
        // TODO: Retry the HEAD request if it fails with a retryable error.
        let (updated_uri, head) = self.request(Method::HEAD, uri, headers.clone()).await;

        if let Ok(response) = head {
            Head::create_inner(
                response.status(),
                response.headers(),
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
    ) -> (Option<Uri>, Result<Self::Response, Error>);
}

pub trait Response {
    /// Return the HTTP `StatusCode` for this request.
    fn status(&self) -> StatusCode;
    /// Return headers for the response.
    fn headers(&self) -> &HeaderMap;
    /// Fetch the next chunk of the response body.
    async fn chunk(&mut self, uri: &Uri) -> Result<Option<Bytes>, Error>;
}
