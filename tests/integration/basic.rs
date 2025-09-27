use best_file_downloader::{Client, ProgressData};
use temp_dir::TempDir;

use crate::integration::constants::SERVER_URL;

const MESSAGE: &str = "hello world";

#[tokio::test]
async fn should_download_a_file() -> Result<(), Box<dyn std::error::Error>> {
    let url = format!("{SERVER_URL}/hello.txt");
    let dir = TempDir::new()?;
    let destination = dir.path().join("my-file.txt");

    let client = Client::new();
    let result = client
        .download(&url)
        .destination(&destination)
        .progress(|data: &ProgressData| {
            println!(
                "Downloaded {} of {} bytes",
                data.bytes(),
                data.total_bytes().unwrap()
            );
        })
        .download()
        .await?;

    assert_eq!(&result.path, &destination);
    let file_contents = tokio::fs::read_to_string(&result.path).await?;
    assert_eq!(file_contents, MESSAGE);
    assert_eq!(result.bytes_downloaded, MESSAGE.len() as u64);

    Ok(())
}

#[tokio::test]
async fn should_fail_on_404() -> Result<(), Box<dyn std::error::Error>> {
    let url = format!("{SERVER_URL}/i.do.not.exist");
    let dir = TempDir::new()?;
    let destination = dir.path().join("my-file.txt");

    // TODO: Verify we don't retry.
    let client = Client::new();
    let result = client
            .download(&url)
            .destination(&destination)
            .download()
            .await;

    let err = result.err().unwrap();
    assert_eq!(format!("{}", err), "Unexpected response status: 404");

    Ok(())
}
