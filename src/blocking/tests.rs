use std::{fs};

use httptest::{
    Expectation, Server,
    matchers::{contains, request},
    responders,
};
use temp_dir::TempDir;

use crate::ClientBuilder;

use super::*;

#[test]
fn should_download_a_file() -> Result<(), Box<dyn std::error::Error>> {
    let message = "hello world";

    let dir = TempDir::new()?;

    // Configure the server to expect a single GET /foo request and respond
    // with a 200 status code.
    let server = Server::run();
    server.expect(
        Expectation::matching(request::method_path("GET", "/file.txt"))
            .respond_with(responders::status_code(200).body(message)),
    );

    let client = Client::new();
    let url = server.url("/file.txt");
    let destination = dir.path().join("my-file.txt");
    let result = client.get(url).destination(destination).send()?;

    assert_eq!(result.bytes_downloaded, 11);

    let file_contents = fs::read_to_string(&result.path)?;
    assert_eq!(file_contents, message);

    Ok(())
}

#[test]
fn should_add_custom_headers() -> Result<(), Box<dyn std::error::Error>> {
    let message = "hello world";

    let dir = TempDir::new()?;

    // Configure the server to expect a single GET /foo request and respond
    // with a 200 status code.
    let server = Server::run();
    server.expect(
        Expectation::matching(httptest::all_of![
            request::method_path("GET", "/file.txt"),
            request::headers(contains(("x-my-header", "potato"))),
            request::headers(contains(("x-my-other", "canon"))),
        ])
        .respond_with(responders::status_code(200).body(message)),
    );

    let client = ClientBuilder::new()
        .header("x-my-header", "potato")
        .blocking()?;
    let url = server.url("/file.txt");
    let destination = dir.path().join("my-file.txt");
    let result = client
        .get(url)
        .header("x-my-other", "canon")
        .destination(destination)
        .send()?;

    assert_eq!(result.bytes_downloaded, 11);

    let file_contents = fs::read_to_string(&result.path)?;
    assert_eq!(file_contents, message);

    Ok(())
}
