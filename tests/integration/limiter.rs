use std::time::{Duration, SystemTime};

use downlow::Client;
use temp_dir::TempDir;

use crate::integration::{constants::SERVER_URL, utils};

#[tokio::test]
async fn should_limit_download_speed() -> Result<(), Box<dyn std::error::Error>> {
    let file_size = 10 * 1024 * 1024; // 10 MB.
    let limit = 5 * 1024 * 1024; // 5 MB/s
    let timeout = Duration::from_secs(file_size as u64 / limit * 2);

    let dir = TempDir::new()?;
    let destination = dir.path().join("my-file.bin");
    let url = format!("{SERVER_URL}{}", utils::big_file_url(file_size));

    // Download the file with a rate limit.
    let client = Client::builder()
        .max_bytes_per_second(limit)
        .build()
        .unwrap();
    let start = SystemTime::now();
    let fut = client.download(&url).destination(&destination).download();
    let result = tokio::time::timeout(timeout, fut).await??;
    let elapsed = start.elapsed().unwrap().as_millis();

    let rate = result.bytes_downloaded as f64 / (elapsed as f64 / 1000.0);
    println!(
        "Downloaded {} bytes in {elapsed} ms ({rate:.2} bytes/sec)",
        result.bytes_downloaded
    );

    assert!(
        (1900..=2100).contains(&elapsed),
        "Download was not rate limited. 2000 ms expected, got {elapsed} ms"
    );

    Ok(())
}

#[tokio::test]
async fn should_change_download_speed_partway_through() -> Result<(), Box<dyn std::error::Error>> {
    let file_size = 10 * 1024 * 1024; // 10 MB.
    let limit = 1024; // Stupid slow.

    let dir = TempDir::new()?;
    let destination = dir.path().join("my-file.bin");
    let url = format!("{SERVER_URL}{}", utils::big_file_url(file_size));

    // Download the file with a rate limit.
    let client = Client::builder()
        .max_bytes_per_second(limit)
        .build()
        .unwrap();
    let start = SystemTime::now();
    let fut = {
        let client = client.clone();
        tokio::spawn(async move {
            client
                .download(&url)
                .destination(&destination)
                .download()
                .await
        })
    };

    // Wait a bit, then increase the limit.
    tokio::time::sleep(Duration::from_secs(1)).await;
    client.max_bytes_per_second(Some(5 * 1024 * 1024)).await;

    let timeout = Duration::from_secs(10);
    tokio::time::timeout(timeout, fut).await???;
    let elapsed = start.elapsed().unwrap().as_millis();

    println!("Downloaded tool {elapsed} ms",);
    // 1 second of downloading slowly, then 2 seconds at 5mb/s.
    assert!(
        (2900..=3100).contains(&elapsed),
        "Download was not rate limited. 3000 ms expected, got {elapsed} ms"
    );

    Ok(())
}

#[tokio::test]
async fn should_remove_download_speed_partway_through() -> Result<(), Box<dyn std::error::Error>> {
    let file_size = 10 * 1024 * 1024; // 10 MB.
    let limit = 1024; // Stupid slow.

    let dir = TempDir::new()?;
    let destination = dir.path().join("my-file.bin");
    let url = format!("{SERVER_URL}{}", utils::big_file_url(file_size));

    // Download the file with a rate limit.
    let client = Client::builder()
        .max_bytes_per_second(limit)
        .build()
        .unwrap();
    let start = SystemTime::now();
    let fut = {
        let client = client.clone();
        tokio::spawn(async move {
            client
                .download(&url)
                .destination(&destination)
                .download()
                .await
        })
    };

    // Wait a bit, then increase the limit.
    tokio::time::sleep(Duration::from_secs(1)).await;
    client.max_bytes_per_second(None).await;

    let timeout = Duration::from_secs(10);
    tokio::time::timeout(timeout, fut).await???;
    let elapsed = start.elapsed().unwrap().as_millis();

    println!("Downloaded tool {elapsed} ms",);
    assert!(
        (900..=1500).contains(&elapsed),
        "Download was not rate limited. 3000 ms expected, got {elapsed} ms"
    );

    Ok(())
}
