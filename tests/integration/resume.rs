use std::time::SystemTime;

use downlow::Client;
use temp_dir::TempDir;

use crate::integration::{constants::SERVER_URL, utils};

const MESSAGE: &str = "hello world";

#[tokio::test]
async fn should_continue_a_file() -> Result<(), Box<dyn std::error::Error>> {
    let url = format!("{SERVER_URL}/hello.txt");
    let dir = TempDir::new()?;
    let destination = dir.path().join("my-file.txt");

    // Create a partial file to simulate a previous download.
    let part_file = dir.path().join("my-file.txt.part");
    tokio::fs::write(&part_file, &MESSAGE[..5]).await?;
    let (last_modified, _) = utils::head_url(&url).await;

    let client = Client::new();
    let result = client
        .download(&url)
        .last_modified(last_modified.unwrap().into())
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

    assert_eq!(&result.path, &destination);

    let file_contents = tokio::fs::read_to_string(&result.path).await?;
    assert_eq!(file_contents, MESSAGE);

    assert_eq!(result.bytes_downloaded, 6);

    Ok(())
}

#[tokio::test]
async fn should_continue_a_file_from_sidecar() -> Result<(), Box<dyn std::error::Error>> {
    let url = format!("{SERVER_URL}/hello.txt");
    let dir = TempDir::new()?;
    let destination = dir.path().join("my-file.txt");

    // Create a partial file to simulate a previous download.
    let part_file = dir.path().join("my-file.txt.part");
    tokio::fs::write(&part_file, &MESSAGE[..5]).await?;
    let (last_modified, etag) = utils::head_url(&url).await;
    let sidecar_file = dir.path().join("my-file.txt.downloadinfo");
    tokio::fs::write(
        &sidecar_file,
        format!(
            "Last-Modified: {last_modified}\nEtag: {etag}\n",
            last_modified = last_modified.unwrap().to_rfc3339(),
            etag = etag.unwrap(),
        ),
    )
    .await?;

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

    assert_eq!(&result.path, &destination);

    // Verify the contents of the file.
    let file_contents = tokio::fs::read_to_string(&result.path).await?;
    assert_eq!(file_contents, MESSAGE);

    // Verify we only downloaded the remaining bytes.
    assert_eq!(result.bytes_downloaded, 6);

    // Verify the sidecar file was deleted.
    assert!(!sidecar_file.exists());

    Ok(())
}

#[tokio::test]
async fn should_not_continue_a_modified_file_from_sidecar() -> Result<(), Box<dyn std::error::Error>>
{
    let url = format!("{SERVER_URL}/hello.txt");
    let dir = TempDir::new()?;
    let destination = dir.path().join("my-file.txt");

    // Create a partial file to simulate a previous download.
    let part_file = dir.path().join("my-file.txt.part");
    tokio::fs::write(&part_file, "abcde").await?;
    let sidecar_file = dir.path().join("my-file.txt.downloadinfo");
    tokio::fs::write(
        &sidecar_file,
        "Last-Modified: 2020-01-01T12:00:00Z\nEtag: wrong\n",
    )
    .await?;

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

    assert_eq!(&result.path, &destination);

    // Verify the contents of the file.
    let file_contents = tokio::fs::read_to_string(&result.path).await?;
    assert_eq!(file_contents, MESSAGE);

    // Verify we only downloaded the remaining bytes.
    assert_eq!(result.bytes_downloaded, MESSAGE.len() as u64);

    // Verify the sidecar file was deleted.
    assert!(!sidecar_file.exists());

    Ok(())
}

#[tokio::test]
async fn should_not_continue_a_file_with_wrong_last_modified()
-> Result<(), Box<dyn std::error::Error>> {
    let url = format!("{SERVER_URL}/hello.txt");
    let dir = TempDir::new()?;
    let destination = dir.path().join("my-file.txt");

    // Create a partial file to simulate a previous download, but don't set the last modified time.
    let part_file = dir.path().join("my-file.txt.part");
    tokio::fs::write(&part_file, &MESSAGE[..5]).await?;

    let client = Client::new();
    let result = client
        .download(&url)
        .destination(&destination)
        .last_modified(SystemTime::UNIX_EPOCH) // definitely wrong
        .on_progress(|progress| {
            println!(
                "Downloaded {} of {} bytes",
                progress.bytes(),
                progress.total_bytes().unwrap()
            );
        })
        .download()
        .await?;

    let file_contents = tokio::fs::read_to_string(&result.path).await?;
    assert_eq!(file_contents, MESSAGE);
    // Should download the whole file again.
    assert_eq!(result.bytes_downloaded, MESSAGE.len() as u64);

    Ok(())
}

#[tokio::test]
async fn should_prefer_etag_over_last_modified() -> Result<(), Box<dyn std::error::Error>> {
    let url = format!("{SERVER_URL}/hello.txt");
    let dir = TempDir::new()?;
    let destination = dir.path().join("my-file.txt");
    let (_, etag) = utils::head_url(&url).await;

    // Create a partial file to simulate a previous download, but don't set the last modified time.
    let part_file = dir.path().join("my-file.txt.part");
    tokio::fs::write(&part_file, &MESSAGE[..5]).await?;

    let client = Client::new();
    let result = client
        .download(&url)
        .destination(&destination)
        .etag(etag.unwrap())
        .last_modified(SystemTime::UNIX_EPOCH) // definitely wrong
        .on_progress(|progress| {
            println!(
                "Downloaded {} of {} bytes",
                progress.bytes(),
                progress.total_bytes().unwrap()
            );
        })
        .download()
        .await?;

    let file_contents = tokio::fs::read_to_string(&result.path).await?;
    assert_eq!(file_contents, MESSAGE);

    // Verify we only downloaded the remaining bytes.
    assert_eq!(result.bytes_downloaded, 6);

    Ok(())
}
