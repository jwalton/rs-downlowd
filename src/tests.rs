use httptest::{
    Expectation, Server,
    matchers::{contains, request},
    responders,
};
use temp_dir::TempDir;

use super::*;

#[tokio::test]
async fn should_download_a_file() -> Result<(), Box<dyn std::error::Error>> {
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
    let result = client
        .download(url)
        .destination(destination)
        .download()
        .await?;

    assert_eq!(result.bytes_downloaded, 11);

    let file_contents = tokio::fs::read_to_string(&result.path).await?;
    assert_eq!(file_contents, message);

    Ok(())
}

#[tokio::test]
async fn should_continue_a_file() -> Result<(), Box<dyn std::error::Error>> {
    let message = "hello world";

    let dir = TempDir::new()?;

    // Configure the server to expect a single GET /foo request and respond
    // with a 200 status code.
    let server = Server::run();
    server.expect(
        Expectation::matching(httptest::all_of![
            request::method_path("GET", "/file.txt"),
            request::headers(contains(("range", "bytes=5-"))),
        ])
        .respond_with(
            responders::status_code(206)
                .append_header("etag", "test-etag")
                .body(&message[5..]),
        ),
    );

    let client = Client::new();
    let url = server.url("/file.txt");
    let destination = dir.path().join("my-file.txt");

    // Create a partial file to simulate a previous download.
    let part_file = dir.path().join("my-file.txt.part");
    tokio::fs::write(&part_file, &message[..5]).await?;

    let result = client
        .download(&url)
        .destination(destination)
        .etag("test-etag")
        .download()
        .await?;

    assert_eq!(result.bytes_downloaded, 6);

    let file_contents = tokio::fs::read_to_string(&result.path).await?;
    assert_eq!(file_contents, message);

    Ok(())
}

#[tokio::test]
async fn should_add_custom_headers() -> Result<(), Box<dyn std::error::Error>> {
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
        .build()?;
    let url = server.url("/file.txt");
    let destination = dir.path().join("my-file.txt");
    let result = client
        .download(url)
        .header("x-my-other", "canon")
        .destination(destination)
        .download()
        .await?;

    assert_eq!(result.bytes_downloaded, 11);

    let file_contents = tokio::fs::read_to_string(&result.path).await?;
    assert_eq!(file_contents, message);

    Ok(())
}
