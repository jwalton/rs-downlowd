use downlowd::Client;
use temp_dir::TempDir;
use tokio::fs;

use crate::integration::{
    constants::SERVER_URL,
    utils::{self, write_sidecar_file, ProgressRecord, ProgressRecorder},
};

const MESSAGE: &str = "hello world";

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
        .get(&url)
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
    )?;

    let client = Client::new();
    let result = client
        .get(&url)
        .destination(&destination)
        .on_progress(|progress| {
            println!(
                "Downloaded {} of {} bytes",
                progress.bytes(),
                progress.remote_length().unwrap()
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
    )?;

    let client = Client::new();
    let result = client
        .get(&url)
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
    )?;

    let client = Client::new();
    let result = client
        .get(&url)
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

/// We can get into a situation where we rename a file, and then before we delete
/// the sidecar file, the program crashes or our future is cancelled.
#[tokio::test]
async fn should_continue_a_file_from_sidecar_that_is_already_complete_and_renamed()
-> Result<(), Box<dyn std::error::Error>> {
    let url = format!("{SERVER_URL}/hello.txt");
    let dir = TempDir::new()?;
    let destination = dir.path().join("my-file.txt");

    // Create a partial file to simulate a previous download.
    tokio::fs::write(dir.path().join("my-file.txt"), &MESSAGE).await?;
    let head = utils::head_url(&url);
    let sidecar_file = dir.path().join("my-file.txt.downloadinfo");
    write_sidecar_file(
        &sidecar_file,
        Some(&head.last_modified),
        Some(&head.etag),
        Some(head.content_length),
    )?;

    let result = Client::new()
        .get(&url)
        .destination(&destination)
        .on_progress(|progress| {
            println!(
                "Downloaded {} of {} bytes",
                progress.bytes(),
                progress.remote_length().unwrap()
            );
        })
        .send()
        .await?;

    assert_eq!(&result.path, &destination);
    let file_contents = tokio::fs::read_to_string(&result.path).await?;
    assert_eq!(file_contents, MESSAGE);
    assert_eq!(result.bytes_downloaded, 0);
    assert!(!sidecar_file.exists());

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
        .get(&url)
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
        .get(&url)
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
    )?;

    let client = Client::new();
    let result = client
        .get(&url)
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
