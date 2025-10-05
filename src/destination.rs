use std::path::PathBuf;

use crate::utils;

pub struct Destination {
    /// Path to the final filename for the file.
    pub path: PathBuf,
    /// Path to the temporary file used during download.
    pub part_file: PathBuf,
    /// Path to the sidecar file used to store metadata about the download.
    pub sidecar_file: PathBuf,
}

impl Destination {
    pub fn new(path: PathBuf) -> Self {
        let part_file = utils::file::add_extension(&path, "part");
        let sidecar_file = utils::file::add_extension(&path, "downloadinfo");
        Self {
            path,
            part_file,
            sidecar_file,
        }
    }
}