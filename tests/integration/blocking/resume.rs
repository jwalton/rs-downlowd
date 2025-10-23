use std::fs;

use downlowd::blocking::Client;
use temp_dir::TempDir;

use crate::integration::{
    constants::SERVER_URL,
    utils::{self, ProgressRecord, ProgressRecorder, write_sidecar_file},
};

const MESSAGE: &str = "hello world";

#[test]
fn should_continue_a_file() -> Result<(), Box<dyn std::error::Error>> {
    let url = format!("{SERVER_URL}/hello.txt");
    let dir = TempDir::new()?;
    let destination = dir.path().join("my-file.txt");

    // Create a partial file to simulate a previous download.
    let part_file = dir.path().join("my-file.txt.part");
    fs::write(&part_file, &MESSAGE[..5])?;
    let head = utils::head_url(&url);

    let recorder = ProgressRecorder::new();

    let client = Client::new();
    let result = client
        .get(&url)
        .last_modified(head.last_modified)
        .destination(&destination)
        .on_progress(recorder.on_progress())
        .send()?;

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

    let file_contents = fs::read_to_string(&result.path)?;
    assert_eq!(file_contents, MESSAGE);

    assert_eq!(result.bytes_downloaded, 6);

    Ok(())
}

#[test]
fn should_continue_a_file_from_sidecar() -> Result<(), Box<dyn std::error::Error>> {
    let url = format!("{SERVER_URL}/hello.txt");
    let dir = TempDir::new()?;
    let destination = dir.path().join("my-file.txt");

    // Create a partial file to simulate a previous download.
    let part_file = dir.path().join("my-file.txt.part");
    fs::write(&part_file, &MESSAGE[..5])?;
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
        .send()?;

    assert_eq!(&result.path, &destination);

    // Verify the contents of the file.
    let file_contents = fs::read_to_string(&result.path)?;
    assert_eq!(file_contents, MESSAGE);

    // Verify we only downloaded the remaining bytes.
    assert_eq!(result.bytes_downloaded, 6);

    // Verify the sidecar file was deleted.
    assert!(!sidecar_file.exists());

    Ok(())
}

#[test]
fn should_not_continue_a_file_from_sidecar_if_length_etag_changed()
-> Result<(), Box<dyn std::error::Error>> {
    let url = format!("{SERVER_URL}/hello.txt");
    let dir = TempDir::new()?;
    let destination = dir.path().join("my-file.txt");

    // Create a partial file to simulate a previous download.
    let part_file = dir.path().join("my-file.txt.part");
    fs::write(&part_file, "abcde")?;
    let head = utils::head_url(&url);
    let sidecar_file = dir.path().join("my-file.txt.downloadinfo");
    write_sidecar_file(
        &sidecar_file,
        Some(&head.last_modified),
        Some("wrong"),
        Some(head.content_length),
    )?;

    let client = Client::new();
    let result = client.get(&url).destination(&destination).send()?;

    assert_eq!(&result.path, &destination);

    // Verify the contents of the file.
    let file_contents = fs::read_to_string(&result.path)?;
    assert_eq!(file_contents, MESSAGE);

    // Verify we only downloaded the remaining bytes.
    assert_eq!(result.bytes_downloaded, MESSAGE.len() as u64);

    // Verify the sidecar file was deleted.
    assert!(!sidecar_file.exists());

    Ok(())
}

#[test]
fn should_not_continue_a_file_from_sidecar_if_length_has_changed()
-> Result<(), Box<dyn std::error::Error>> {
    let url = format!("{SERVER_URL}/hello.txt");
    let dir = TempDir::new()?;
    let destination = dir.path().join("my-file.txt");

    // Create a partial file to simulate a previous download.
    let part_file = dir.path().join("my-file.txt.part");
    fs::write(&part_file, "---")?;
    let head = utils::head_url(&url);
    let sidecar_file = dir.path().join("my-file.txt.downloadinfo");
    write_sidecar_file(
        &sidecar_file,
        Some(&head.last_modified),
        Some(&head.etag),
        Some(5),
    )?;

    let client = Client::new();
    let result = client.get(&url).destination(&destination).send()?;

    assert_eq!(&result.path, &destination);

    // Verify the contents of the file.
    let file_contents = fs::read_to_string(&result.path)?;
    assert_eq!(file_contents, MESSAGE);
    assert_eq!(result.bytes_downloaded, 11);

    Ok(())
}

#[test]
fn should_not_continue_a_file_with_wrong_last_modified() -> Result<(), Box<dyn std::error::Error>> {
    let url = format!("{SERVER_URL}/hello.txt");
    let dir = TempDir::new()?;
    let destination = dir.path().join("my-file.txt");

    // Create a partial file to simulate a previous download.
    let part_file = dir.path().join("my-file.txt.part");
    fs::write(&part_file, &MESSAGE[..5])?;

    let client = Client::new();
    let result = client
        .get(&url)
        .destination(&destination)
        .last_modified("wrong")
        .send()?;

    let file_contents = fs::read_to_string(&result.path)?;
    assert_eq!(file_contents, MESSAGE);
    // Should download the whole file again.
    assert_eq!(result.bytes_downloaded, MESSAGE.len() as u64);

    Ok(())
}

#[test]
fn should_redownload_if_etag_is_same_but_last_modified_has_changed()
-> Result<(), Box<dyn std::error::Error>> {
    let url = format!("{SERVER_URL}/hello.txt");
    let dir = TempDir::new()?;
    let destination = dir.path().join("my-file.txt");
    let head = utils::head_url(&url);

    // Create a partial file to simulate a previous download.
    let part_file = dir.path().join("my-file.txt.part");
    fs::write(&part_file, &MESSAGE[..5])?;

    let client = Client::new();
    let result = client
        .get(&url)
        .destination(&destination)
        .etag(head.etag)
        .last_modified("wrong")
        .send()?;

    let file_contents = fs::read_to_string(&result.path)?;
    assert_eq!(file_contents, MESSAGE);

    // Verify we only downloaded the remaining bytes.
    assert_eq!(result.bytes_downloaded, 11);

    Ok(())
}

#[test]
fn should_prefer_user_etag_over_sidecar_file() -> Result<(), Box<dyn std::error::Error>> {
    let url = format!("{SERVER_URL}/hello.txt");
    let dir = TempDir::new()?;
    let destination = dir.path().join("my-file.txt");

    // Create a partial file to simulate a previous download.
    let part_file = dir.path().join("my-file.txt.part");
    fs::write(&part_file, &MESSAGE[..5])?;
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
        .send()?;

    assert_eq!(&result.path, &destination);

    // Verify the contents of the file.
    let file_contents = fs::read_to_string(&result.path)?;
    assert_eq!(file_contents, MESSAGE);

    // Verify we only downloaded the remaining bytes.
    assert_eq!(result.bytes_downloaded, 6);

    // Verify the sidecar file was deleted.
    assert!(!sidecar_file.exists());

    Ok(())
}
