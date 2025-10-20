use std::{fs, time::Duration};

use downlowd::blocking::Client;
use http::HeaderMap;
use temp_dir::TempDir;

use crate::integration::{
    constants::SERVER_URL,
    utils::{self, ProgressRecord, ProgressRecorder},
};

const MESSAGE: &str = "hello world";

#[test]
fn should_get_the_name_of_the_file() -> Result<(), Box<dyn std::error::Error>> {
    let url = format!("{SERVER_URL}/hello.txt");

    let mut download = Client::new().get(&url);
    assert_eq!(download.get_remote_file_name(), "hello.txt");
    Ok(())
}

#[test]
fn should_download_a_file() -> Result<(), Box<dyn std::error::Error>> {
    let url = format!("{SERVER_URL}/hello.txt");
    let dir = TempDir::new()?;
    let destination = dir.path();

    let head = utils::head_url(&url);

    let recorder = ProgressRecorder::new();

    let client = Client::new();
    let result = {
        let recorder = recorder.clone();
        client
            .get(&url)
            .destination(destination)
            .on_progress(move |p| {
                assert_eq!(p.etag().unwrap(), head.etag);
                assert_eq!(p.last_modified().unwrap(), head.last_modified);
                assert_eq!(p.remote_length().unwrap(), head.content_length);

                recorder.record_progress(p);
            })
            .send()?
    };

    assert_eq!(
        recorder.records(),
        &[
            ProgressRecord {
                bytes: 0,
                total_bytes: Some(11)
            },
            ProgressRecord {
                bytes: 11,
                total_bytes: Some(11)
            }
        ]
    );

    assert_eq!(&result.path, &destination.join("hello.txt"));
    let file_contents = fs::read_to_string(&result.path)?;
    assert_eq!(file_contents, MESSAGE);
    assert_eq!(result.bytes_downloaded, MESSAGE.len() as u64);

    Ok(())
}

#[test]
fn should_skip_an_already_downloaded_file() -> Result<(), Box<dyn std::error::Error>> {
    let url = format!("{SERVER_URL}/hello.txt");
    let dir = TempDir::new()?;
    let destination = dir.path().join("my-file.txt");

    fs::write(&destination, MESSAGE)?;

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
    let file_contents = fs::read_to_string(&result.path)?;
    assert_eq!(file_contents, MESSAGE);
    assert_eq!(result.bytes_downloaded, 0);

    Ok(())
}

#[test]
fn should_not_skip_a_file_if_the_size_is_wrong() -> Result<(), Box<dyn std::error::Error>> {
    let url = format!("{SERVER_URL}/hello.txt");
    let dir = TempDir::new()?;
    let destination = dir.path().join("my-file.txt");

    fs::write(&destination, "a")?;

    let client = Client::new();
    let result = client.get(&url).destination(&destination).send()?;

    let file_contents = fs::read_to_string(&result.path)?;
    assert_eq!(file_contents, MESSAGE);

    Ok(())
}

#[test]
fn should_fail_on_404() -> Result<(), Box<dyn std::error::Error>> {
    let url = format!("{SERVER_URL}/i.do.not.exist");
    let dir = TempDir::new()?;
    let destination = dir.path().join("my-file.txt");

    let client = Client::new();
    let result = client
        .get(&url)
        .destination(&destination)
        .on_retry(move |_| {
            panic!("Should not retry on 404");
        })
        .on_progress(move |_| {
            panic!("Should not call progress handler on 404");
        })
        .send();

    let err = result.err().unwrap();
    assert_eq!(format!("{}", err), "Unexpected response status: 404");

    Ok(())
}

#[test]
fn should_allow_cancelling_a_download() -> Result<(), Box<dyn std::error::Error>> {
    let url = format!("{SERVER_URL}{}", utils::big_file_url(10 * 1024 * 1024));
    let dir = TempDir::new()?;
    let destination = dir.path().join("my-file.bin");
    let part_file = dir.path().join("my-file.bin.part");

    let client = Client::new();
    let result = client
        .get(&url)
        .destination(&destination)
        .on_progress(|progress| {
            if progress.bytes() > 1_000_000 {
                println!("Cancelling download after {} bytes", progress.bytes());
                progress.cancel();
            }
        })
        .send()
        .unwrap_err();

    println!("Error: {result} for {url}");
    assert!(matches!(result, downlowd::Error::Cancelled));
    let file_size = fs::metadata(&part_file)?.len();
    println!("file_size: {file_size}");
    assert!(file_size > 1_000_000);
    assert!(file_size < 10 * 1024 * 1024);

    // Continue the download.
    let result = client.get(&url).destination(&destination).send()?;

    assert_eq!(&result.path, &destination);
    let file_size = fs::metadata(&destination)?.len();
    assert_eq!(file_size, 10 * 1024 * 1024);

    Ok(())
}

#[test]
fn should_allow_setting_all_the_settings() -> Result<(), Box<dyn std::error::Error>> {
    let url = format!("{SERVER_URL}/hello.txt");

    let client = Client::builder()
        .user_agent("my-user-agent/1.0")
        .header("key", "value")
        .headers(HeaderMap::new())
        .max_retries(Some(10))
        .max_bytes_per_second(Some(1024 * 1024))
        .build()?;

    // Update the client settings.
    client.max_bytes_per_second(Some(1024 * 1024));

    let _download = client
        .get(url)
        .user_agent("my-user-agent-2/1.0")
        .header("key", "new-value")
        .headers(HeaderMap::new())
        .on_progress(|progress| {
            println!(
                "Downloaded {} of {} bytes",
                progress.bytes(),
                progress.remote_length().unwrap()
            );
        })
        .on_retry(|r| r.set_delay(Duration::from_secs(1)))
        .max_retries(Some(20))
        .etag("some-etag")
        .last_modified("2023-10-01T12:34:56Z");

    Ok(())
}
