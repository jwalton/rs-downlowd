use std::time::SystemTime;

use downlowd::Client;
use temp_dir::TempDir;
use tokio::fs;

use crate::integration::utils::{self, web_server::spawn_web_server_async, is_ci};

const ITERATIONS: u32 = 10;

#[tokio::test]
async fn should_be_fast() -> Result<(), Box<dyn std::error::Error>> {
    let dir = TempDir::new()?;
    let destination = dir.path().join("my-file.bin");
    let server_url = spawn_web_server_async().await;
    let url = format!("{server_url}{}", utils::big_file_url(10 * 1024 * 1024));

    // This is the "naive" approach where we download the whole file into memory
    // using reqwest, then write it to disk.
    let start = SystemTime::now();
    for _ in 0..ITERATIONS {
        let _ = fs::remove_file(&destination).await;
        let response = reqwest::get(&url).await?;
        let bytes = response.bytes().await?;
        tokio::fs::write(&destination, &bytes).await?;
    }
    let elapsed = start.elapsed().unwrap();
    let control_ms_per_iteration = elapsed.as_millis() as f64 / ITERATIONS as f64;

    // Then, the same, but using downlowd to do the download.
    let client = Client::new();
    let start = SystemTime::now();
    for _ in 0..ITERATIONS {
        let _ = fs::remove_file(&destination).await;
        client.get(&url).destination(&destination).send().await?;
    }
    let elapsed = start.elapsed().unwrap();
    let ms_per_iteration = elapsed.as_millis() as f64 / ITERATIONS as f64;

    // Higher is worse.
    let ratio = ms_per_iteration / control_ms_per_iteration;
    println!("Naive time: {control_ms_per_iteration:.2} ms per iteration");
    println!("Our time:   {ms_per_iteration:.2} ms per iteration");
    println!("Ratio:      {ratio:.2} (lower is better)");

    // This should be quite low, but on github actions, we're in a VM and performance
    // measurements can vary wildly.
    let target = if is_ci() { 2.0 } else { 1.5 };
    assert!(ratio < target, "Download was too slow");

    Ok(())
}
