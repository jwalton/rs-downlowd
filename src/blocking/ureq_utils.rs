use std::str::FromStr;

use http::{HeaderMap, Response, Uri};
use ureq::{Agent, Body, ResponseExt};
use url::Url;

use crate::{Error, utils::into_url::IntoUrlSealed};

/// Make a request using ureq.
pub fn ureq_request(
    agent: &Agent,
    method: http::Method,
    url: &Url,
    headers: &HeaderMap,
) -> (Option<Url>, Result<Response<Body>, Error>) {
    let Ok(uri) = Uri::from_str(url.as_str()) else {
        return (
            None,
            Err(Error::InvalidUrl {
                cause: "could not convert url to uri".to_string(),
            }),
        );
    };

    let method_name = match method {
        http::Method::GET => "GET",
        http::Method::HEAD => "HEAD",
        _ => "REQUEST",
    };

    let mut request = http::Request::builder().uri(&uri).method(method);
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
            url: url.to_string(),
            cause: err.to_string(),
        },
    });

    let response = match response {
        Ok(r) => r,
        Err(e) => {
            return (None, Err(e));
        }
    };

    let returned_url = if response.get_uri() != &uri {
        response.get_uri().to_string().into_url().ok()
    } else {
        None
    };

    (returned_url, Ok(response))
}
