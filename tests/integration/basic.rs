use std::sync::{Arc, atomic::AtomicU64};

use best_file_downloader::{Client, ProgressData};
use temp_dir::TempDir;

use crate::integration::{constants::SERVER_URL, utils};

const MESSAGE: &str = "hello world";

#[tokio::test]
async fn should_download_a_file() -> Result<(), Box<dyn std::error::Error>> {
    let url = format!("{SERVER_URL}/hello.txt");
    let dir = TempDir::new()?;
    let destination = dir.path().join("my-file.txt");

    let client = Client::new();
    let result = client
        .download(&url)
        .destination(&destination)
        .progress(|data: &mut ProgressData| {
            println!(
                "Downloaded {} of {} bytes",
                data.bytes(),
                data.total_bytes().unwrap()
            );
        })
        .download()
        .await?;

    assert_eq!(&result.path, &destination);
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
        .progress(|data: &mut ProgressData| {
            println!(
                "Downloaded {} of {} bytes",
                data.bytes(),
                data.total_bytes().unwrap()
            );
        })
        .download()
        .await?;

    assert_eq!(result.status, best_file_downloader::Status::Skipped);
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

    assert_eq!(result.status, best_file_downloader::Status::Downloaded);
    let file_contents = tokio::fs::read_to_string(&result.path).await?;
    assert_eq!(file_contents, MESSAGE);

    Ok(())
}

#[tokio::test]
async fn should_fail_on_404() -> Result<(), Box<dyn std::error::Error>> {
    let url = format!("{SERVER_URL}/i.do.not.exist");
    let dir = TempDir::new()?;
    let destination = dir.path().join("my-file.txt");

    let progress_event_count = Arc::new(AtomicU64::new(0));

    // TODO: Verify we don't retry.
    let client = Client::new();
    let result = {
        let progress_event_count = progress_event_count.clone();
        client
            .download(&url)
            .destination(&destination)
            .progress(move |_: &mut ProgressData| {
                progress_event_count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            })
            .download()
            .await
    };

    let err = result.err().unwrap();
    assert_eq!(format!("{}", err), "Unexpected response status: 404");

    // We should have made only a single attempt and failed right away, so there
    // should be no progress events at all.
    assert_eq!(
        progress_event_count.load(std::sync::atomic::Ordering::SeqCst),
        0
    );

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
        .progress(|data: &mut ProgressData| {
            if data.bytes() > 1_000_000 {
                println!("Cancelling download after {} bytes", data.bytes());
                data.cancel();
            }
        })
        .download()
        .await.unwrap_err();

    println!("Error: {result} for {url}");
    assert!(matches!(result, best_file_downloader::Error::Cancelled));
    let file_size = tokio::fs::metadata(&part_file).await?.len();
    assert!(file_size > 1_000_000);
    assert!(file_size < 10 * 1024 * 1024);

    // Continue the download.
    let result = client
        .download(&url)
        .destination(&destination)
        .download()
        .await?;

    assert_eq!(result.status, best_file_downloader::Status::Downloaded);
    assert_eq!(&result.path, &destination);
    let file_size = tokio::fs::metadata(&destination).await?.len();
    assert_eq!(file_size, 10 * 1024 * 1024);

    Ok(())
}
