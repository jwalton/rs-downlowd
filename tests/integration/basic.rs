use downlow::Client;
use temp_dir::TempDir;

use crate::integration::{constants::SERVER_URL, utils};

const MESSAGE: &str = "hello world";

#[tokio::test]
async fn should_download_a_file() -> Result<(), Box<dyn std::error::Error>> {
    let url = format!("{SERVER_URL}/hello.txt");
    let dir = TempDir::new()?;
    let destination = dir.path();

    let head = utils::head_url(&url).await;

    let client = Client::new();
    let result = client
        .download(&url)
        .destination(destination)
        .on_progress(move |progress| {
            assert_eq!(progress.etag().unwrap(), head.etag);
            assert_eq!(progress.last_modified().unwrap(), head.last_modified.into());
            assert_eq!(progress.total_bytes().unwrap(), head.content_length);

            println!(
                "Downloaded {} of {} bytes",
                progress.bytes(),
                progress.total_bytes().unwrap()
            );
        })
        .download()
        .await?;

    assert_eq!(&result.path, &destination.join("hello.txt"));
    let file_contents = tokio::fs::read_to_string(&result.path).await?;
    assert_eq!(file_contents, MESSAGE);
    assert_eq!(result.bytes_downloaded, MESSAGE.len() as u64);

    Ok(())
}

#[tokio::test]
async fn should_skip_an_already_downloaded_file() -> Result<(), Box<dyn std::error::Error>> {
    let url = format!("{SERVER_URL}/hello.txt");
    let dir = TempDir::new()?;
    let destination = dir.path().join("my-file.txt");

    tokio::fs::write(&destination, MESSAGE).await?;

    let client = Client::new();
    let result = client
        .download(&url)
        .destination(&destination)
        .on_progress(|progress| {
            println!(
                "Downloaded {} of {} bytes",
                progress.bytes(),
                progress.total_bytes().unwrap()
            );
        })
        .download()
        .await?;

    assert_eq!(result.status, downlow::Status::Skipped);
    assert_eq!(&result.path, &destination);
    let file_contents = tokio::fs::read_to_string(&result.path).await?;
    assert_eq!(file_contents, MESSAGE);
    assert_eq!(result.bytes_downloaded, 0);

    Ok(())
}

#[tokio::test]
async fn should_not_skip_a_file_if_the_size_is_wrong() -> Result<(), Box<dyn std::error::Error>> {
    let url = format!("{SERVER_URL}/hello.txt");
    let dir = TempDir::new()?;
    let destination = dir.path().join("my-file.txt");

    tokio::fs::write(&destination, "a").await?;

    let client = Client::new();
    let result = client
        .download(&url)
        .destination(&destination)
        .download()
        .await?;

    assert_eq!(result.status, downlow::Status::Downloaded);
    let file_contents = tokio::fs::read_to_string(&result.path).await?;
    assert_eq!(file_contents, MESSAGE);

    Ok(())
}

#[tokio::test]
async fn should_fail_on_404() -> Result<(), Box<dyn std::error::Error>> {
    let url = format!("{SERVER_URL}/i.do.not.exist");
    let dir = TempDir::new()?;
    let destination = dir.path().join("my-file.txt");

    let client = Client::new();
    let result = client
        .download(&url)
        .destination(&destination)
        .on_retry(move |_| {
            panic!("Should not retry on 404");
        })
        .on_progress(move |_| {
            panic!("Should not call progress handler on 404");
        })
        .download()
        .await;

    let err = result.err().unwrap();
    assert_eq!(format!("{}", err), "Unexpected response status: 404");

    Ok(())
}

#[tokio::test]
async fn should_allow_cancelling_a_download() -> Result<(), Box<dyn std::error::Error>> {
    let url = format!("{SERVER_URL}{}", utils::big_file_url());
    let dir = TempDir::new()?;
    let destination = dir.path().join("my-file.bin");
    let part_file = dir.path().join("my-file.bin.part");

    let client = Client::new();
    let result = client
        .download(&url)
        .destination(&destination)
        .on_progress(|progress| {
            if progress.bytes() > 1_000_000 {
                println!("Cancelling download after {} bytes", progress.bytes());
                progress.cancel();
            }
        })
        .download()
        .await
        .unwrap_err();

    println!("Error: {result} for {url}");
    assert!(matches!(result, downlow::Error::Cancelled));
    let file_size = tokio::fs::metadata(&part_file).await?.len();
    println!("file_size: {file_size}");
    assert!(file_size > 1_000_000);
    assert!(file_size < 10 * 1024 * 1024);

    // Continue the download.
    let result = client
        .download(&url)
        .destination(&destination)
        .download()
        .await?;

    assert_eq!(result.status, downlow::Status::Downloaded);
    assert_eq!(&result.path, &destination);
    let file_size = tokio::fs::metadata(&destination).await?.len();
    assert_eq!(file_size, 10 * 1024 * 1024);

    Ok(())
}
