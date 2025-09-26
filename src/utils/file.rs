//! This is a file for miscellaneous file-related utilities.  We want to be able
//! to use this in either sync or async contexts.  Since tokio's file operations
//! generally are thin wrappers around the sync versions, these utilities are all
//! written using sync code, and then these get wrapped in blocking tasks when
//! used in async contexts.

use std::{
    io::Seek,
    path::{Path, PathBuf},
};

use tokio::io::AsyncSeekExt;

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
pub fn open_file_for_writing(part_file: &Path) -> Result<(std::fs::File, u64), Error> {
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

    let metadata = file.metadata().map_err(|e| Error::Write {
        action: "getting file metadata",
        path: part_file.to_path_buf(),
        cause: e,
    })?;

    Ok((file, metadata.len()))
}

pub async fn open_file_for_writing_async(
    part_file: &Path,
) -> Result<(tokio::fs::File, u64), Error> {
    let part_file = part_file.to_owned();
    let (file, len) = tokio::task::spawn_blocking(move || open_file_for_writing(&part_file))
        .await
        .unwrap()?;
    Ok((tokio::fs::File::from_std(file), len))
}

/// Truncate the file to zero length and seek to the start.
pub async fn truncate_file_async(filename: &Path, file: &mut tokio::fs::File) -> Result<(), Error> {
    file.set_len(0).await.map_err(|e| Error::Write {
        action: "truncating file",
        path: filename.to_path_buf(),
        cause: e,
    })?;
    file.seek(std::io::SeekFrom::Start(0))
        .await
        .map_err(|e| Error::Write {
            action: "seeking to start of file",
            path: filename.to_path_buf(),
            cause: e,
        })?;
    Ok(())
}
