use http::HeaderMap;
use url::Url;

use crate::Error;

/// Send a request to the server, following redirects as necessary.  Returns the
/// URL we actually fetched from, and the response (or an error).
pub async fn request(
    client: &reqwest::Client,
    method: reqwest::Method,
    url: Url,
    headers: HeaderMap,
) -> (Option<Url>, Result<reqwest::Response, Error>) {
    let method_name = match method {
        reqwest::Method::GET => "GET",
        reqwest::Method::HEAD => "HEAD",
        _ => "REQUEST",
    };

    // Reqwest follows redirect automatically.
    let response = client
        .request(method, url.clone())
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
                    url: url.to_string(),
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

    let returned_url = if response.url() != &url {
        Some(response.url().clone())
    } else {
        None
    };

    (returned_url, Ok(response))
}
