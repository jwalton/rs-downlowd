use http::{HeaderMap, Response, Uri};
use ureq::{Agent, Body, ResponseExt};

use crate::Error;

/// Make a request using ureq.
pub fn ureq_request(
    agent: &Agent,
    method: http::Method,
    uri: &Uri,
    headers: &HeaderMap,
) -> (Option<Uri>, Result<Response<Body>, Error>) {
    let method_name = match method {
        http::Method::GET => "GET",
        http::Method::HEAD => "HEAD",
        _ => "REQUEST",
    };

    let mut request = http::Request::builder().uri(uri).method(method);
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

    let response = agent.run(request).map_err(|err| match err {
        ureq::Error::RedirectFailed => Error::BadRedirect {
            reason: "redirect failed",
        },
        ureq::Error::TooManyRedirects => Error::BadRedirect {
            reason: "too many redirects",
        },
        _ => Error::Network {
            during: method_name,
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

    (returned_url, Ok(response))
}
