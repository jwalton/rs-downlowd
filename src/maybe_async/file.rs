use std::{fs::Metadata, path::Path};

use crate::Error;

pub trait File: Sized {
    /// Open a file for writing.
    ///
    /// This opens the specified path, and obtains a lock to the file to ensure
    /// no other process or task can write to the file until  we are done with it.
    async fn open_for_writing(path: &Path) -> Result<Self, Error>;

    /// Read the contents of a file to a string.
    async fn read_to_string(path: &Path) -> Result<String, std::io::Error>;

    /// Get metadata for a file.
    async fn metadata(path: &Path) -> Result<Metadata, Error>;

    /// Remove a file.
    async fn remove_file(path: &Path) -> Result<(), Error>;

    /// Get the lenght of this file.
    async fn get_length(&self) -> Result<u64, Error>;

    /// Truncate this file to 0 bytes.
    async fn truncate(&mut self) -> Result<(), Error>;

    /// Rename this file.
    async fn rename(&mut self, path: &Path) -> Result<(), Error>;

    /// Flush all writes to the file.
    async fn sync_all(&mut self) -> Result<(), Error>;

    /// Write everything in the buffer to the file.
    async fn write_all<'a>(&'a mut self, src: &'a [u8]) -> Result<(), Error>;
}
