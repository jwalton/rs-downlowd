//! This is a file for miscellaneous file-related utilities.  We want to be able
//! to use this in either sync or async contexts.  Since tokio's file operations
//! generally are thin wrappers around the sync versions, these utilities are all
//! written using sync code, and then these get wrapped in blocking tasks when
//! used in async contexts.

use std::path::{Path, PathBuf};

use crate::Error;

// Add an extension to a path. If the path already has an extension, the new
// extension is appended to the existing one, separated by a dot.  (e.g. if you
// add "part" to "file.tar", you get "file.tar.part")
pub fn add_extension(path: &Path, extension: &str) -> PathBuf {
    let mut new_path = path.to_owned();
    match new_path.extension() {
        Some(ext) => {
            new_path.set_extension(format!("{}.{}", ext.to_string_lossy(), extension));
        }
        None => {
            new_path.set_extension(extension);
        }
    }
    new_path
}

/// Open the destination file for writing.  If the file already exists, this will
/// open the file for appending and return the current length of the file.
///
pub fn open_file_for_writing(part_file: &Path) -> Result<std::fs::File, Error> {
    // Make sure the parent directory exists.
    if let Some(parent) = part_file.parent() {
        std::fs::create_dir_all(parent).map_err(|e| Error::Write {
            action: "creating directory",
            path: parent.to_path_buf(),
            cause: e,
        })?;
    }

    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(part_file)
        .map_err(|e| Error::Write {
            action: "opening file for writing",
            path: part_file.to_path_buf(),
            cause: e,
        })?;

    // Get an exclusive lock to the file, to make sure no one else is writing to it.
    file.lock().map_err(|e| Error::Write {
        action: "locking file",
        path: part_file.to_path_buf(),
        cause: e,
    })?;

    Ok(file)
}