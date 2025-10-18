use std::path::Path;

use downlowd::Client;
use temp_dir::TempDir;
use tokio::fs;

use crate::integration::{
    constants::SERVER_URL,
    utils::{self, ProgressRecord, ProgressRecorder},
};

const MESSAGE: &str = "hello world";

async fn write_sidecar_file(
    path: &Path,
    last_modified: Option<&str>,
    etag: Option<&str>,
    content_length: Option<u64>,
) -> std::io::Result<()> {
    let mut contents = String::new();
    if let Some(last_modified) = last_modified {
        contents.push_str(&format!("Last-Modified: {last_modified}\n"));
    }
    if let Some(etag) = etag {
        contents.push_str(&format!("Etag: {etag}\n",));
    }
    if let Some(content_length) = content_length {
        contents.push_str(&format!("File-Length: {content_length}\n"));
    }
    fs::write(path, contents).await?;
    Ok(())
}

#[tokio::test]
async fn should_continue_a_file() -> Result<(), Box<dyn std::error::Error>> {
    let url = format!("{SERVER_URL}/hello.txt");
    let dir = TempDir::new()?;
    let destination = dir.path().join("my-file.txt");

    // Create a partial file to simulate a previous download.
    let part_file = dir.path().join("my-file.txt.part");
    fs::write(&part_file, &MESSAGE[..5]).await?;
    let head = utils::head_url(&url);

    let recorder = ProgressRecorder::new();

    let client = Client::new();
    let result = client
        .download(&url)
        .last_modified(head.last_modified)
        .destination(&destination)
        .on_progress(recorder.on_progress())
        .send()
        .await?;

    {
        assert_eq!(
            recorder.records(),
            &[
                ProgressRecord {
                    bytes: 5,
                    total_bytes: Some(11)
                },
                ProgressRecord {
                    bytes: 11,
                    total_bytes: Some(11)
                }
            ]
        );
    }

    assert_eq!(&result.path, &destination);

    let file_contents = fs::read_to_string(&result.path).await?;
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
    fs::write(&part_file, &MESSAGE[..5]).await?;
    let head = utils::head_url(&url);
    let sidecar_file = dir.path().join("my-file.txt.downloadinfo");
    write_sidecar_file(
        &sidecar_file,
        Some(&head.last_modified),
        Some(&head.etag),
        Some(head.content_length),
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
        .send()
        .await?;

    assert_eq!(&result.path, &destination);

    // Verify the contents of the file.
    let file_contents = fs::read_to_string(&result.path).await?;
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
    fs::write(&part_file, "abcde").await?;
    let head = utils::head_url(&url);
    let sidecar_file = dir.path().join("my-file.txt.downloadinfo");
    write_sidecar_file(
        &sidecar_file,
        Some(&head.last_modified),
        Some("wrong"),
        Some(head.content_length),
    )
    .await?;

    let client = Client::new();
    let result = client
        .download(&url)
        .destination(&destination)
        .send()
        .await?;

    assert_eq!(&result.path, &destination);

    // Verify the contents of the file.
    let file_contents = fs::read_to_string(&result.path).await?;
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
    fs::write(&part_file, "---").await?;
    let head = utils::head_url(&url);
    let sidecar_file = dir.path().join("my-file.txt.downloadinfo");
    write_sidecar_file(
        &sidecar_file,
        Some(&head.last_modified),
        Some(&head.etag),
        Some(5),
    )
    .await?;

    let client = Client::new();
    let result = client
        .download(&url)
        .destination(&destination)
        .send()
        .await?;

    assert_eq!(&result.path, &destination);

    // Verify the contents of the file.
    let file_contents = fs::read_to_string(&result.path).await?;
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

    // Create a partial file to simulate a previous download.
    let part_file = dir.path().join("my-file.txt.part");
    fs::write(&part_file, &MESSAGE[..5]).await?;

    let client = Client::new();
    let result = client
        .download(&url)
        .destination(&destination)
        .last_modified("wrong")
        .send()
        .await?;

    let file_contents = fs::read_to_string(&result.path).await?;
    assert_eq!(file_contents, MESSAGE);
    // Should download the whole file again.
    assert_eq!(result.bytes_downloaded, MESSAGE.len() as u64);

    Ok(())
}

#[tokio::test]
async fn should_redownload_if_etag_is_same_but_last_modified_has_changed()
-> Result<(), Box<dyn std::error::Error>> {
    let url = format!("{SERVER_URL}/hello.txt");
    let dir = TempDir::new()?;
    let destination = dir.path().join("my-file.txt");
    let head = utils::head_url(&url);

    // Create a partial file to simulate a previous download.
    let part_file = dir.path().join("my-file.txt.part");
    fs::write(&part_file, &MESSAGE[..5]).await?;

    let client = Client::new();
    let result = client
        .download(&url)
        .destination(&destination)
        .etag(head.etag)
        .last_modified("wrong")
        .send()
        .await?;

    let file_contents = fs::read_to_string(&result.path).await?;
    assert_eq!(file_contents, MESSAGE);

    // Verify we only downloaded the remaining bytes.
    assert_eq!(result.bytes_downloaded, 11);

    Ok(())
}

#[tokio::test]
async fn should_prefer_user_etag_over_sidecar_file() -> Result<(), Box<dyn std::error::Error>> {
    let url = format!("{SERVER_URL}/hello.txt");
    let dir = TempDir::new()?;
    let destination = dir.path().join("my-file.txt");

    // Create a partial file to simulate a previous download.
    let part_file = dir.path().join("my-file.txt.part");
    fs::write(&part_file, &MESSAGE[..5]).await?;
    let head = utils::head_url(&url);
    let sidecar_file = dir.path().join("my-file.txt.downloadinfo");
    write_sidecar_file(
        &sidecar_file,
        Some(&head.last_modified),
        Some("wrong"),
        Some(head.content_length),
    )
    .await?;

    let client = Client::new();
    let result = client
        .download(&url)
        .destination(&destination)
        // We're providing the correct etag, maybe from a database.  This should
        // override whatever the sidecar file says.
        .etag(head.etag)
        .send()
        .await?;

    assert_eq!(&result.path, &destination);

    // Verify the contents of the file.
    let file_contents = fs::read_to_string(&result.path).await?;
    assert_eq!(file_contents, MESSAGE);

    // Verify we only downloaded the remaining bytes.
    assert_eq!(result.bytes_downloaded, 6);

    // Verify the sidecar file was deleted.
    assert!(!sidecar_file.exists());

    Ok(())
}
