use std::{
    sync::mpsc,
    thread,
    time::{Duration, SystemTime},
};

use downlowd::blocking::Client;
use temp_dir::TempDir;

use crate::integration::{constants::SERVER_URL, utils};

pub fn with_timeout<F, T>(timeout: Duration, f: F) -> T
where
    F: FnOnce() -> T,
    F: Send + 'static,
    T: Send + 'static,
{
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || tx.send(f()));
    rx.recv_timeout(timeout).unwrap()
}

#[test]
fn should_limit_download_speed() -> Result<(), Box<dyn std::error::Error>> {
    let file_size = 5 * 1024 * 1024; // 5 MB.
    let limit = 5 * 1024 * 1024; // 5 MB/s
    let timeout = Duration::from_secs(file_size as u64 / limit * 2);

    let dir = TempDir::new()?;
    let destination = dir.path().join("my-file.bin");
    let url = format!("{SERVER_URL}{}", utils::big_file_url(file_size));

    let start = SystemTime::now();

    let result = with_timeout(timeout, move || {
        // Download the file with a rate limit.
        let client = Client::builder()
            .max_bytes_per_second(Some(limit))
            .build()
            .unwrap();
        client.get(&url).destination(&destination).send()
    })?;

    let elapsed = start.elapsed().unwrap().as_millis();

    let rate = result.bytes_downloaded as f64 / (elapsed as f64 / 1000.0);
    println!(
        "Downloaded {} bytes in {elapsed} ms ({rate:.2} bytes/sec)",
        result.bytes_downloaded
    );

    let expeted = 1000;
    assert!(
        (expeted - 300..=expeted + 300).contains(&elapsed),
        "Download was not rate limited. {expeted} ms expected, got {elapsed} ms"
    );

    Ok(())
}

#[test]
fn should_change_download_speed_partway_through() -> Result<(), Box<dyn std::error::Error>> {
    let file_size = 10 * 1024 * 1024; // 10 MB.
    let limit = 32 * 1024; // 32 KB/s.

    let dir = TempDir::new()?;
    let destination = dir.path().join("my-file.bin");
    let url = format!("{SERVER_URL}{}", utils::big_file_url(file_size));

    // Download the file with a rate limit.
    let client = Client::builder()
        .max_bytes_per_second(Some(limit))
        .build()
        .unwrap();
    let start = SystemTime::now();
    let join_handle = {
        let client = client.clone();
        thread::spawn(move || client.get(&url).destination(&destination).send())
    };

    // Wait a bit, then increase the limit.
    thread::sleep(Duration::from_millis(500));
    client.max_bytes_per_second(Some(10 * 1024 * 1024));

    with_timeout(Duration::from_secs(10), move || join_handle.join()).unwrap()?;
    let elapsed = start.elapsed().unwrap().as_millis();

    println!("Downloaded took {elapsed} ms",);

    // 1 second of downloading slowly, then 2 seconds at 5mb/s.
    assert!(
        (1400..=1800).contains(&elapsed),
        "Download was not rate limited. 1500 ms expected, got {elapsed} ms"
    );

    Ok(())
}

#[test]
fn should_allow_removing_download_speed_partway_through() -> Result<(), Box<dyn std::error::Error>>
{
    let file_size = 10 * 1024 * 1024; // 10 MB.
    let limit = 32 * 1024; // 32 KB/s.

    let dir = TempDir::new()?;
    let destination = dir.path().join("my-file.bin");
    let url = format!("{SERVER_URL}{}", utils::big_file_url(file_size));

    // Download the file with a rate limit.
    let client = Client::builder()
        .max_bytes_per_second(Some(limit))
        .build()
        .unwrap();
    let start = SystemTime::now();
    let join_handle = {
        let client = client.clone();
        thread::spawn(move || client.get(&url).destination(&destination).send())
    };

    // Wait a bit, then increase the limit.
    thread::sleep(Duration::from_secs(1));
    client.max_bytes_per_second(None);

    with_timeout(Duration::from_secs(10), move || join_handle.join()).unwrap()?;
    let elapsed = start.elapsed().unwrap().as_millis();

    println!("Downloaded took {elapsed} ms",);
    assert!(
        (900..=1500).contains(&elapsed),
        "Download was not rate limited. 1000 ms expected, got {elapsed} ms"
    );

    Ok(())
}
