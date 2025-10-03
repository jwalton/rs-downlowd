use std::{collections::HashMap, io::Write, sync::{Mutex, OnceLock}};

use rand::Fill;

mod progress_recorder;
pub use progress_recorder::*;

pub struct HeadData {
    pub last_modified: String,
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
        .map(|s| s.to_str().expect("valid string").to_owned())
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

static FILES: OnceLock<Mutex<HashMap<usize, String>>> = OnceLock::new();

/// Returns the URL path to a file of a specific size for testing purposes.
/// The file is created on first use in the `test-support/static` directory.
pub fn big_file_url(size: usize) -> String {
    let file_map = FILES.get_or_init(|| Mutex::new(HashMap::new()));

    let mut map = file_map.lock().unwrap();
    if let Some(path) = map.get(&size) {
        return path.clone();
    }

    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("test-support")
        .join("static")
        .join(format!("big-file-{size}.bin"));

    // Check if the file already exists and is the correct size.
    let file_size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0) as usize;
    if file_size != size {
        let mut file = std::fs::File::create(&path).expect("create file");

        // Write random data to the file.
        let mut remaining = size;
        let mut buffer = [0u8; 4096];
        let mut rng = rand::rng();
        while remaining > 0 {
            buffer.fill(&mut rng);
            let buffer = &buffer[..remaining.min(buffer.len())];
            file.write_all(buffer).expect("write to file");
            remaining -= buffer.len();
        }
    }

    let url = format!("/big-file-{size}.bin");
    map.insert(size, url.clone());
    url
}
