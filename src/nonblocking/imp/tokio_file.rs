use std::{
    fs::Metadata, io::{self, SeekFrom}, path::{Path, PathBuf}
};

use tokio::{
    fs,
    io::{AsyncSeekExt, AsyncWriteExt},
};

use crate::{Error, maybe_async, utils};

pub struct TokioFile {
    path: PathBuf,
    inner: tokio::fs::File,
}

impl TokioFile {
    fn convert_error(&self, err: io::Error, action: &'static str) -> Error {
        Error::Write {
            action,
            path: self.path.clone(),
            cause: err,
        }
    }
}

impl maybe_async::File for TokioFile {
    async fn open_for_writing(path: &Path) -> Result<Self, Error> {
        let path = path.to_owned();
        tokio::task::spawn_blocking(move || {
            let inner = utils::file::open_file_for_writing(&path)?;
            Ok(Self {
                path,
                inner: inner.into(),
            })
        })
        .await
        .unwrap()
    }

    async fn read_to_string(path: &Path) -> Result<String, std::io::Error> {
        fs::read_to_string(path).await
    }

    async fn metadata(path: &Path) -> Result<Metadata, Error> {
        fs::metadata(path).await.map_err(|e| Error::Write {
            action: "getting file metadata",
            path: path.to_path_buf(),
            cause: e,
        })
    }

    async fn remove_file(path: &std::path::Path) -> Result<(), Error> {
        fs::remove_file(path).await.map_err(|e| Error::Write {
            action: "removing file",
            path: path.to_path_buf(),
            cause: e,
        })
    }

    async fn get_length(&self) -> Result<u64, Error> {
        self.inner
            .metadata()
            .await
            .map_err(|e| self.convert_error(e, "reading length"))
            .map(|m| m.len())
    }

    async fn sync_all(&mut self) -> Result<(), Error> {
        self.inner
            .sync_all()
            .await
            .map_err(|err| self.convert_error(err, "sync_all"))
    }

    async fn rename(&mut self, path: &Path) -> Result<(), Error> {
        fs::rename(&self.path, path)
            .await
            .map_err(|err| self.convert_error(err, "renaming file"))
    }

    async fn truncate(&mut self) -> Result<(), Error> {
        self.inner
            .set_len(0)
            .await
            .map_err(|err| self.convert_error(err, "truncating file"))?;
        self.inner
            .seek(SeekFrom::Start(0))
            .await
            .map_err(|err| self.convert_error(err, "seeking to start of file"))?;
        Ok(())
    }

    async fn write_all<'a>(&'a mut self, buf: &'a [u8]) -> Result<(), Error> {
        self.inner
            .write_all(buf)
            .await
            .map_err(|err| self.convert_error(err, "writing to file"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use temp_dir::TempDir;

    #[tokio::test]
    async fn should_truncate_a_file() -> Result<(), Box<dyn std::error::Error>> {
        use crate::maybe_async::File;

        let dir = TempDir::new()?;
        let path = dir.path().join("my-file.txt");

        let mut file = TokioFile::open_for_writing(&path).await?;
        file.write_all(b"Hello, world!").await?;
        file.sync_all().await?;

        let len = file.get_length().await?;
        assert_eq!(len, 13);

        file.truncate().await?;

        let len = file.get_length().await?;
        assert_eq!(len, 0);

        Ok(())
    }
}
