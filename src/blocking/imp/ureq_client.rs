use std::io::Read;

use ::http::{HeaderMap, Method, Uri};
use bytes::Bytes;
use ureq::{Agent, ResponseExt};

use crate::{Error, maybe_async};

const BUFFER_SIZE: usize = 128 * 1024;

#[derive(Clone)]
pub struct UreqClient {
    agent: Agent,
}

pub struct Response {
    inner: ::http::Response<ureq::Body>,
}

impl maybe_async::Response for Response {
    fn status(&self) -> http::StatusCode {
        self.inner.status()
    }

    fn headers(&self) -> &HeaderMap {
        self.inner.headers()
    }

    async fn chunk(&mut self, uri: &Uri) -> Result<Option<Bytes>, Error> {
        let mut buf = [0; BUFFER_SIZE];
        let mut reader = self.inner.body_mut().as_reader();

        let n = reader.read(&mut buf).map_err(|cause| Error::Network {
            uri: uri.to_string(),
            cause: cause.to_string(),
        })?;
        if n == 0 {
            return Ok(None);
        }

        let chunk = Bytes::from_owner(buf);
        return Ok(Some(chunk.slice(0..n)));
    }
}

impl UreqClient {
    pub fn new(agent: Agent) -> Self {
        Self { agent }
    }
}

impl maybe_async::Client for UreqClient {
    type Response = Response;

    async fn request(
        &self,
        method: Method,
        uri: &Uri,
        headers: HeaderMap,
    ) -> (Option<Uri>, Result<Self::Response, crate::Error>) {
        let mut request = ::http::Request::builder().uri(uri).method(method);
        if let Some(h) = request.headers_mut() {
            for (k, v) in headers.iter() {
                h.append(k, v.clone());
            }
        }
        let request = match request.body(()) {
            Err(e) => {
                return (
                    None,
                    Err(Error::InvalidHeader {
                        cause: e.to_string(),
                    }),
                );
            }
            Ok(r) => r,
        };

        let response = self.agent.run(request).map_err(|err| match err {
            ureq::Error::RedirectFailed => Error::BadRedirect {
                reason: "redirect failed",
            },
            ureq::Error::TooManyRedirects => Error::BadRedirect {
                reason: "too many redirects",
            },
            _ => Error::Network {
                uri: uri.to_string(),
                cause: err.to_string(),
            },
        });

        let response = match response {
            Ok(r) => r,
            Err(e) => {
                return (None, Err(e));
            }
        };

        let returned_url = if response.get_uri() != uri {
            Some(response.get_uri().to_owned())
        } else {
            None
        };

        let response = Response { inner: response };

        (returned_url, Ok(response))
    }
}
