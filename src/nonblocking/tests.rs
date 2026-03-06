use std::time::Duration;

use http::header;
use httptest::{
    Expectation, Server,
    matchers::{contains, request},
    responders,
};
use temp_dir::TempDir;

use crate::{ClientBuilder, Error};

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
    let result = client.get(url).destination(destination).send().await?;

    assert_eq!(result.bytes_downloaded, 11);

    let file_contents = tokio::fs::read_to_string(&result.path).await?;
    assert_eq!(file_contents, message);

    Ok(())
}

#[tokio::test]
async fn should_set_the_user_agent_in_client() -> Result<(), Box<dyn std::error::Error>> {
    let message = "hello world";
    let dir = TempDir::new()?;
    let server = Server::run();

    server.expect(
        Expectation::matching(httptest::all_of![
            request::method_path("GET", "/file.txt"),
            request::headers(contains(("user-agent", "test1"))),
        ])
        .respond_with(responders::status_code(200).body(message)),
    );

    let client = Client::builder().user_agent("test1").build()?;
    let result = client
        .get(server.url("/file.txt"))
        .destination(dir.path().join("my-file.txt"))
        .send()
        .await?;

    assert_eq!(result.bytes_downloaded, 11);
    Ok(())
}

#[tokio::test]
async fn should_set_the_user_agent_for_a_single_download() -> Result<(), Box<dyn std::error::Error>>
{
    let message = "hello world";
    let dir = TempDir::new()?;
    let server = Server::run();

    server.expect(
        Expectation::matching(httptest::all_of![
            request::method_path("GET", "/file.txt"),
            request::headers(contains(("user-agent", "test2"))),
        ])
        .respond_with(responders::status_code(200).body(message)),
    );

    let client = Client::builder().user_agent("test1").build()?;
    let result = client
        .get(server.url("/file.txt"))
        .user_agent("test2")
        .destination(dir.path().join("my-file.txt"))
        .send()
        .await?;

    assert_eq!(result.bytes_downloaded, 11);
    Ok(())
}

#[tokio::test]
async fn should_follow_redirects() -> Result<(), Box<dyn std::error::Error>> {
    let message = "hello world";

    let dir = TempDir::new()?;

    // Configure the server to expect a single GET /foo request and respond
    // with a 200 status code.
    let server = Server::run();
    server.expect(
        Expectation::matching(request::method_path("GET", "/file.txt"))
            .respond_with(responders::status_code(301).append_header("Location", "/file2.txt")),
    );
    server.expect(
        Expectation::matching(request::method_path("GET", "/file2.txt"))
            .respond_with(responders::status_code(200).body(message)),
    );

    let client = Client::new();
    let url = server.url("/file.txt");
    let redirect_url = server.url("/file2.txt");
    let destination = dir.path().join("my-file.txt");
    let result = client
        .get(url.clone())
        .destination(destination)
        .on_progress(move |progress| {
            // Verify the progress handler calims to have followed the redirect.
            assert_eq!(progress.original_uri().to_string(), url.to_string());
            assert_eq!(progress.uri().to_string(), redirect_url.to_string());
        })
        .send()
        .await?;

    assert_eq!(result.bytes_downloaded, 11);

    let file_contents = tokio::fs::read_to_string(&result.path).await?;
    assert_eq!(file_contents, message);

    Ok(())
}

#[tokio::test]
async fn should_not_follow_redirect_loop() -> Result<(), Box<dyn std::error::Error>> {
    let dir = TempDir::new()?;

    // Configure the server to expect a single GET /foo request and respond
    // with a 200 status code.
    let server = Server::run();
    server.expect(
        Expectation::matching(request::method_path("GET", "/file.txt"))
            .times(1..)
            .respond_with(responders::status_code(301).append_header("Location", "/file.txt")),
    );

    let client = Client::new();
    let url = server.url("/file.txt");
    let destination = dir.path().join("my-file.txt");
    let result = client
        .get(url)
        .max_retries(Some(100))
        .destination(destination)
        .send()
        .await;

    let err = result.unwrap_err();
    assert!(matches!(err, Error::BadRedirect { .. }));

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
        .get(url)
        .destination(destination)
        .etag("test-etag")
        .send()
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
        .get(url)
        .header("x-my-other", "canon")
        .destination(destination)
        .send()
        .await?;

    assert_eq!(result.bytes_downloaded, 11);

    let file_contents = tokio::fs::read_to_string(&result.path).await?;
    assert_eq!(file_contents, message);

    Ok(())
}

#[tokio::test]
async fn should_retry_a_download() -> Result<(), Box<dyn std::error::Error>> {
    let message = "hello world";

    let dir = TempDir::new()?;

    // Configure the server to expect a single GET /foo request and respond
    // with a 200 status code.
    let server = Server::run();
    server.expect(
        Expectation::matching(request::method_path("GET", "/file.txt"))
            .times(2)
            .respond_with(responders::cycle![
                responders::status_code(500).body("boom"),
                responders::status_code(200).body(message)
            ]),
    );

    let client = Client::new();
    let url = server.url("/file.txt");
    let destination = dir.path().join("my-file.txt");
    let retry_count = std::sync::Arc::new(std::sync::Mutex::new(0));

    let result = {
        let retry_count = retry_count.clone();
        client
            .get(url)
            .destination(destination)
            .on_retry(move |r| {
                let mut count = retry_count.lock().unwrap();
                *count += 1;
                r.set_delay(Duration::ZERO);
            })
            .send()
            .await?
    };

    // Should have retried once.
    assert_eq!(*retry_count.lock().unwrap(), 1);
    // Should have downloaded the file.
    assert_eq!(result.bytes_downloaded, 11);

    let file_contents = tokio::fs::read_to_string(&result.path).await?;
    assert_eq!(file_contents, message);

    Ok(())
}

#[tokio::test]
async fn should_abort_on_retry() -> Result<(), Box<dyn std::error::Error>> {
    let dir = TempDir::new()?;

    // Configure the server to expect a single GET /foo request and respond
    // with a 200 status code.
    let server = Server::run();
    server.expect(
        Expectation::matching(request::method_path("GET", "/file.txt"))
            .respond_with(responders::status_code(500).body("boom")),
    );

    let client = Client::new();
    let url = server.url("/file.txt");
    let destination = dir.path().join("my-file.txt");

    let result = client
        .get(url)
        .destination(destination)
        .on_retry(move |r| {
            r.cancel();
        })
        .send()
        .await;

    assert!(matches!(
        result,
        Err(Error::UnexpectedStatus { status: 500 })
    ));
    Ok(())
}

#[tokio::test]
async fn should_use_an_existing_reqwest_client() -> Result<(), Box<dyn std::error::Error>> {
    let message = "hello world";
    let dir = TempDir::new()?;
    let server = Server::run();

    server.expect(
        Expectation::matching(httptest::all_of![
            request::method_path("GET", "/file.txt"),
            request::headers(contains(("x-from-reqwest", "foo"))),
            request::headers(contains(("x-from-dl", "bar"))),
            request::headers(contains(("user-agent", "downlowd"))),
        ])
        .respond_with(responders::status_code(200).body(message)),
    );

    // Create the reqwest client.
    let mut default_headers = header::HeaderMap::new();
    default_headers.insert("x-from-reqwest", header::HeaderValue::from_static("foo"));
    let client = reqwest::ClientBuilder::new()
        .default_headers(default_headers)
        .user_agent("reqwest")
        .build()?;

    // Create the downlowd client.
    let mut default_headers = header::HeaderMap::new();
    default_headers.insert("x-from-dl", header::HeaderValue::from_static("bar"));
    let client = Client::builder()
        .headers(default_headers)
        .reqwest_client(client)
        .user_agent("downlowd")
        .build()?;

    let result = client
        .get(server.url("/file.txt"))
        .destination(dir.path().join("my-file.txt"))
        .send()
        .await?;

    assert_eq!(result.bytes_downloaded, 11);
    Ok(())
}
