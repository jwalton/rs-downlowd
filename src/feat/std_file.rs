use std::{
    fs::{self, Metadata},
    io::{self, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
};

use crate::{Error, maybe_async, utils};

pub struct StdFile {
    path: PathBuf,
    inner: std::fs::File,
}

impl StdFile {
    fn convert_error(&self, err: io::Error, action: &'static str) -> Error {
        Error::Write {
            action,
            path: self.path.clone(),
            cause: err,
        }
    }
}

impl maybe_async::File for StdFile {
    async fn open_for_writing(path: &Path) -> Result<Self, Error> {
        let inner = utils::file::open_file_for_writing(path)?;
        Ok(Self {
            path: path.to_owned(),
            inner,
        })
    }

    async fn read_to_string(path: &Path) -> Result<String, std::io::Error> {
        fs::read_to_string(path)
    }

    async fn metadata(path: &Path) -> Result<Metadata, Error> {
        path.metadata().map_err(|e| Error::Write {
            action: "getting file metadata",
            path: path.to_path_buf(),
            cause: e,
        })
    }

    async fn remove_file(path: &std::path::Path) -> Result<(), Error> {
        fs::remove_file(path).map_err(|e| Error::Write {
            action: "removing file",
            path: path.to_path_buf(),
            cause: e,
        })
    }

    async fn get_length(&self) -> Result<u64, Error> {
        self.inner
            .metadata()
            .map_err(|e| self.convert_error(e, "reading length"))
            .map(|m| m.len())
    }

    async fn sync_all(&mut self) -> Result<(), Error> {
        self.inner
            .sync_all()
            .map_err(|err| self.convert_error(err, "sync_all"))
    }

    async fn rename(&mut self, path: &Path) -> Result<(), Error> {
        fs::rename(&self.path, path).map_err(|err| self.convert_error(err, "renaming file"))
    }

    async fn truncate(&mut self) -> Result<(), Error> {
        self.inner
            .set_len(0)
            .map_err(|err| self.convert_error(err, "truncating file"))?;
        self.inner
            .seek(SeekFrom::Start(0))
            .map_err(|err| self.convert_error(err, "seeking to start of file"))?;
        Ok(())
    }

    async fn write_all<'a>(&'a mut self, buf: &'a [u8]) -> Result<(), Error> {
        self.inner
            .write_all(buf)
            .map_err(|err| self.convert_error(err, "writing to file"))
    }
}
