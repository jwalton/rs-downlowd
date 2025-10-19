use http::{HeaderMap, Uri};

use crate::Error;

/// Send a request to the server, following redirects as necessary.  Returns the
/// URL we actually fetched from, and the response (or an error).
pub async fn request(
    client: &reqwest::Client,
    method: reqwest::Method,
    uri: &Uri,
    headers: HeaderMap,
) -> (Option<Uri>, Result<reqwest::Response, Error>) {
    let url = uri.to_string();
    let method_name = match method {
        reqwest::Method::GET => "GET",
        reqwest::Method::HEAD => "HEAD",
        _ => "REQUEST",
    };

    // Reqwest follows redirect automatically.
    let response = client
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

    (returned_uri, Ok(response))
}
