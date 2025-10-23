use std::{
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use downlowd::Client;
use temp_dir::TempDir;

use crate::integration::{constants::SERVER_URL, utils};

/// This test case repeatedly creates and cancels a download to ensure that
/// there are no timing issues or cancel-safety problems.
#[tokio::test]
async fn should_work_when_cancelled() -> Result<(), Box<dyn std::error::Error>> {
    let file_size = 3 * 1024 * 1024; // 3 MB.
    let dir = TempDir::new()?;
    let destination = dir.path().join("my-file.bin");
    let url = format!("{SERVER_URL}{}", utils::big_file_url(file_size));

    // Download the file with a rate limit.
    let client = Client::builder()
        .max_bytes_per_second(Some(1024 * 1024)) // 1 MB/s
        .build()
        .unwrap();

    let bytes_downloaded = Arc::new(AtomicU64::new(0));
    let mut attempts = 0;
    loop {
        attempts += 1;
        let bytes_downloaded = bytes_downloaded.clone();
        let fut = client
            .get(&url)
            .destination(&destination)
            .on_progress(move |p| {
                bytes_downloaded.fetch_add(p.delta(), Ordering::SeqCst);
            })
            .send();
        let result = tokio::time::timeout(Duration::from_millis(100), fut).await;

        if result.is_ok() {
            break;
        }
    }

    println!(
        "Completed download after {attempts} attempts: {} bytes downloaded",
        bytes_downloaded.load(Ordering::SeqCst)
    );

    // Make sure the file we downloaded is correct.
    let big_file = utils::big_file_path(file_size);
    let expected_contents = tokio::fs::read(&big_file).await?;
    let actual_contents = tokio::fs::read(&destination).await?;
    assert_eq!(actual_contents.len(), file_size);

    let mut diff_start = None;
    if expected_contents != actual_contents {
        for i in 0..expected_contents.len() {
            if diff_start.is_none() && expected_contents[i] != actual_contents[i] {
                diff_start = Some(i);
            } else if diff_start.is_some() && expected_contents == actual_contents {
                println!("Diff from {} to {i}", diff_start.unwrap());
                diff_start = None;
            }
        }

        if let Some(diff_start) = diff_start {
            println!("Diff from {diff_start} to end");
        }

        let _ = tokio::fs::write("/tmp/expected", expected_contents).await;
        let _ = tokio::fs::write("/tmp/actual", actual_contents).await;
        panic!("Conents are wrong");
    }

    assert_eq!(
        actual_contents.len() as u64,
        bytes_downloaded.load(Ordering::SeqCst)
    );

    Ok(())
}
