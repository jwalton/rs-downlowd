use httptest::{Expectation, Server, matchers::request, responders};

/// Generate a reqwest::Response for testing.
pub async fn make_reqwest_response(
    status: u16,
    headers: &[(&str, &str)],
    message: &str,
) -> reqwest::Response {
    let server = Server::run();

    let mut response = responders::status_code(status).body(message.to_string());
    for (k, v) in headers {
        response = response.append_header(*k, *v);
    }

    server.expect(Expectation::matching(request::method_path("GET", "/")).respond_with(response));

    let client = reqwest::Client::new();
    client
        .get(server.url("/").to_string())
        .send()
        .await
        .unwrap()
}
