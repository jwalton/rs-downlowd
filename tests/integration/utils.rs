use std::{io::Write, sync::OnceLock};

use chrono::{DateTime, Utc};
use rand::Fill;

pub struct HeadData {
    pub last_modified: DateTime<Utc>,
    pub etag: String,
    pub content_length: u64,
}

pub async fn head_url(url: &str) -> HeadData {
    let http_client = reqwest::Client::new();
    let response = http_client.head(url).send().await.expect("url to exist");

    assert_eq!(response.status(), 200, "Resource {url} should exist");

    let last_modified = response
        .headers()
        .get(reqwest::header::LAST_MODIFIED)
        .map(|s| {
            chrono::DateTime::parse_from_rfc2822(s.to_str().expect("valid string"))
                .expect("valid date")
                .with_timezone(&chrono::Utc)
        })
        .unwrap();

    let etag = response
        .headers()
        .get(reqwest::header::ETAG)
        .map(|s| s.to_str().expect("valid string").to_owned())
        .unwrap();

    let content_length = response
        .headers()
        .get(reqwest::header::CONTENT_LENGTH)
        .map(|s| {
            s.to_str()
                .expect("valid string")
                .parse::<u64>()
                .expect("valid u64")
        })
        .unwrap();

    HeadData {
        last_modified,
        etag,
        content_length,
    }
}

static BIG_FILE: OnceLock<&'static str> = OnceLock::new();

/// Returns the URL path to a big file (10 MB) for testing purposes.
/// The file is created on first use in the `test-support/static` directory.
pub fn big_file_url() -> &'static str {
    // Create /test-support/static/big-file.txt if it doesn't already exist.
    BIG_FILE.get_or_init(|| {
        let target_file_size = 10 * 1024 * 1024; // 10 MB

        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("test-support")
            .join("static")
            .join("big-file.txt");

        let file_size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);

        if file_size < target_file_size {
            let mut file = std::fs::File::create(&path).expect("create file");

            // Write 10 MB of random data to the file.
            let mut remaining = 10 * 1024 * 1024;
            let mut buffer = [0u8; 4096];
            let mut rng = rand::rng();
            while remaining > 0 {
                buffer.fill(&mut rng);
                file.write_all(&buffer).expect("write to file");
                remaining -= buffer.len();
            }
        }

        "/big-file.txt"
    })
}
