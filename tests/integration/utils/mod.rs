use std::{
    collections::HashMap, fs, io::Write, path::{Path, PathBuf}, sync::{Mutex, OnceLock}
};

use rand::Fill;

mod progress_recorder;
pub use progress_recorder::*;

pub struct HeadData {
    pub last_modified: String,
    pub etag: String,
    pub content_length: u64,
}

#[derive(Clone)]
pub struct BigFile {
    pub path: PathBuf,
    pub url: String,
}

pub fn head_url(url: &str) -> HeadData {
    let response = ureq::head(url).call().expect("url to exist");

    assert_eq!(response.status(), 200, "Resource {url} should exist");

    let last_modified = response
        .headers()
        .get(http::header::LAST_MODIFIED)
        .map(|s| s.to_str().expect("valid string").to_owned())
        .unwrap();

    let etag = response
        .headers()
        .get(http::header::ETAG)
        .map(|s| s.to_str().expect("valid string").to_owned())
        .unwrap();

    let content_length = response
        .headers()
        .get(http::header::CONTENT_LENGTH)
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

static FILES: OnceLock<Mutex<HashMap<usize, BigFile>>> = OnceLock::new();

fn big_file(size: usize) -> BigFile {
    let file_map = FILES.get_or_init(|| Mutex::new(HashMap::new()));

    let mut map = file_map.lock().unwrap();
    if let Some(bf) = map.get(&size) {
        return bf.clone();
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

    let result = BigFile {
        path,
        url: format!("/big-file-{size}.bin"),
    };
    map.insert(size, result.clone());
    result
}

/// Returns the URL path to a file of a specific size for testing purposes.
/// The file is created on first use in the `test-support/static` directory.
pub fn big_file_url(size: usize) -> String {
    big_file(size).url
}

/// Return the path on the file system to a big file of the specified size.
/// This will be the same file as returned by `big_file_url`.
pub fn big_file_path(size: usize) -> PathBuf {
    big_file(size).path
}

pub fn write_sidecar_file(
    path: &Path,
    last_modified: Option<&str>,
    etag: Option<&str>,
    content_length: Option<u64>,
) -> std::io::Result<()> {
    let mut contents = String::new();
    if let Some(last_modified) = last_modified {
        contents.push_str(&format!("Last-Modified: {last_modified}\n"));
    }
    if let Some(etag) = etag {
        contents.push_str(&format!("Etag: {etag}\n",));
    }
    if let Some(content_length) = content_length {
        contents.push_str(&format!("File-Length: {content_length}\n"));
    }
    fs::write(path, contents)?;
    Ok(())
}
