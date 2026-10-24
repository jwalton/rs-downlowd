//! This is a file for miscellaneous file-related utilities.  We want to be able
//! to use this in either sync or async contexts.  Since tokio's file operations
//! generally are thin wrappers around the sync versions, these utilities are all
//! written using sync code, and then these get wrapped in blocking tasks when
//! used in async contexts.

use std::{
    io::{Seek, SeekFrom},
    path::{Path, PathBuf},
};

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

    // Open file in write mode and then seek to the end of the file (instead
    // of using append mode) because append mode breaks locking on windows.
    // See https://github.com/rust-lang/rust/issues/54118.
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(false)
        .open(part_file)
        .map_err(|e| Error::Write {
            action: "opening file for writing",
            path: part_file.to_path_buf(),
            cause: e,
        })?;

    file.seek(SeekFrom::End(0)).map_err(|e| Error::Write {
        action: "seeking to end of file",
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

pub fn sanitize_filename(filename: &str) -> Option<String> {
    // Split on final "/" or "\" if there is one.
    let filename = filename.rsplit(['/', '\\', ':']).next().unwrap_or(filename);

    let mut filename = sanitize_filename::sanitize_with_options(
        filename,
        sanitize_filename::Options {
            windows: true,
            truncate: true,
            replacement: "",
        },
    );

    // Remove any leading "."s so we can't create hidden files.
    if filename.starts_with('.') {
        filename = filename.trim_start_matches('.').to_string();
    }

    if filename.is_empty() {
        None
    } else {
        Some(filename)
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_sanitize_filename() {
        let check = |input: &str, expected: Option<&str>| {
            let result = sanitize_filename(input);
            assert_eq!(result.as_deref(), expected);
        };

        check("/etc/passwd", Some("passwd"));
        check("../evil", Some("evil"));
        check("c:evil", Some("evil"));
        check("c:\\evil", Some("evil"));
        check("c:/evil", Some("evil"));
        check("平和.txt", Some("平和.txt"));
        check(".hidden", Some("hidden"));
        check("COM1", None);
    }
}