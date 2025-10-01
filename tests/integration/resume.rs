use std::{path::Path, time::SystemTime};

use chrono::{DateTime, Utc};
use downlow::Client;
use temp_dir::TempDir;

use crate::integration::{constants::SERVER_URL, utils};

const MESSAGE: &str = "hello world";

async fn write_sidecar_file(
    path: &Path,
    last_modified: Option<DateTime<Utc>>,
    etag: Option<&str>,
    content_length: Option<u64>,
) -> std::io::Result<()> {
    let mut contents = String::new();
    if let Some(last_modified) = last_modified {
        contents.push_str(&format!("Last-Modified: {}\n", last_modified.to_rfc3339()));
    }
    if let Some(etag) = etag {
        contents.push_str(&format!("Etag: {etag}\n",));
    }
    if let Some(content_length) = content_length {
        contents.push_str(&format!("Content-Length: {content_length}\n"));
    }
    tokio::fs::write(path, contents).await?;
    Ok(())
}

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
    write_sidecar_file(&sidecar_file, last_modified, etag.as_deref(), Some(11)).await?;

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
async fn should_not_continue_a_file_from_sidecar_if_length_etag_changed()
-> Result<(), Box<dyn std::error::Error>> {
    let url = format!("{SERVER_URL}/hello.txt");
    let dir = TempDir::new()?;
    let destination = dir.path().join("my-file.txt");

    // Create a partial file to simulate a previous download.
    let part_file = dir.path().join("my-file.txt.part");
    tokio::fs::write(&part_file, "abcde").await?;
    let (last_modified, _) = utils::head_url(&url).await;
    let sidecar_file = dir.path().join("my-file.txt.downloadinfo");
    write_sidecar_file(&sidecar_file, last_modified, Some("wrong"), None).await?;

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
async fn should_not_continue_a_file_from_sidecar_if_length_has_changed()
-> Result<(), Box<dyn std::error::Error>> {
    let url = format!("{SERVER_URL}/hello.txt");
    let dir = TempDir::new()?;
    let destination = dir.path().join("my-file.txt");

    // Create a partial file to simulate a previous download.
    let part_file = dir.path().join("my-file.txt.part");
    tokio::fs::write(&part_file, "whoop").await?;
    let (last_modified, etag) = utils::head_url(&url).await;
    let sidecar_file = dir.path().join("my-file.txt.downloadinfo");
    write_sidecar_file(&sidecar_file, last_modified, etag.as_deref(), Some(5)).await?;

    let client = Client::new();
    let result = client
        .download(&url)
        .destination(&destination)
        .download()
        .await?;

    assert_eq!(&result.path, &destination);

    // Verify the contents of the file.
    let file_contents = tokio::fs::read_to_string(&result.path).await?;
    assert_eq!(file_contents, MESSAGE);
    assert_eq!(result.bytes_downloaded, 11);

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

#[tokio::test]
async fn should_prefer_user_etag_over_sidecar_file() -> Result<(), Box<dyn std::error::Error>> {
    let url = format!("{SERVER_URL}/hello.txt");
    let dir = TempDir::new()?;
    let destination = dir.path().join("my-file.txt");

    // Create a partial file to simulate a previous download.
    let part_file = dir.path().join("my-file.txt.part");
    tokio::fs::write(&part_file, &MESSAGE[..5]).await?;
    let (last_modified, etag) = utils::head_url(&url).await;
    let sidecar_file = dir.path().join("my-file.txt.downloadinfo");
    write_sidecar_file(&sidecar_file, last_modified, Some("wrong"), None).await?;

    let client = Client::new();
    let result = client
        .download(&url)
        .destination(&destination)
        // We're providing the correct etag, maybe from a database.  This should
        // override whatever the sidecar file says.
        .etag(etag.unwrap())
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
