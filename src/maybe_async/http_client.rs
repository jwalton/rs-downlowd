use bytes::Bytes;
use http::{HeaderMap, Method, StatusCode, Uri};

use crate::Error;

pub trait Client {
    type Response: Response;

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
