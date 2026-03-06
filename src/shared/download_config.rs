use std::path::{Path, PathBuf};

use http::{
    HeaderMap, HeaderValue, Uri,
    header::{self, IntoHeaderName},
};

use crate::{
    Error, IntoUri, Progress, ProgressHandle, RetryHandle, RetryHandler,
    file_info::FileInfo,
    handles::default_retry_callback,
    utils::{
        self,
        http::{append_header, insert_header},
    },
};

/// Represents configuraton used to download a file.
pub struct DownloadConfig {
    /// The URL we want to download from.
    pub uri: Uri,
    /// Headers to include in the request.
    pub headers: HeaderMap,
    /// The configured destination for the file, if any.  This could be a directory
    /// or an actual file.
    pub destination: Option<PathBuf>,
    /// The maximum number of times we will consecutively retry without making progress.
    pub max_retries: Option<u64>,
    /// The callback to call to report progress.
    pub progress_handler: Option<Box<dyn Progress + Send>>,
    /// The handler to call when we retry a download.
    pub retry_handler: RetryHandler,
    /// Information we've been given about the remote file.
    pub user_provided_local_file_info: FileInfo,
    /// If there are any errors while configuring the download, we store them here,
    /// so we can return them when we actually try to start the download.
    pub err: Option<Error>,
}

impl DownloadConfig {
    /// Create a new download for the given URL.
    pub(crate) fn new(url: impl IntoUri, headers: &HeaderMap) -> Self {
        let (url, err) = match url.into_uri() {
            Ok(u) => (u, None),
            Err(e) => (Uri::from_static("http://invalid/"), Some(e)),
        };

        DownloadConfig {
            uri: url,
            headers: headers.clone(),
            destination: None,
            max_retries: None,
            progress_handler: None,
            retry_handler: Box::new(default_retry_callback),
            user_provided_local_file_info: FileInfo::default(),
            err,
        }
    }

    /// Set the user agent for this download.
    pub fn user_agent<V>(&mut self, user_agent: V)
    where
        HeaderValue: TryFrom<V>,
        <HeaderValue as TryFrom<V>>::Error: Into<http::Error>,
    {
        if let Err(e) = insert_header(&mut self.headers, header::USER_AGENT, user_agent) {
            self.err = Some(e);
        }
    }

    /// Add a header to this download.
    pub fn header<K, V>(&mut self, key: K, value: V)
    where
        K: IntoHeaderName,
        HeaderValue: TryFrom<V>,
        <HeaderValue as TryFrom<V>>::Error: Into<http::Error>,
    {
        if let Err(e) = append_header(&mut self.headers, key, value) {
            self.err = Some(e);
        }
    }

    /// Add a set of Headers to the existing ones on this download.
    /// The headers will be merged in to any already set.
    pub fn headers(&mut self, headers: HeaderMap) {
        utils::http::append_all_headers(&mut self.headers, headers);
    }

    /// Override the the maxmimum number of times to consecutively retry the
    /// download without making any progress. Pass in `None` to retry forever.
    pub fn max_retries(&mut self, max_retries: Option<u64>) {
        self.max_retries = max_retries;
    }

    /// Set the progress reporter for this download.  The given reporter will
    /// be called periodically as data is downloaded.
    pub fn on_progress(&mut self, progress: impl FnMut(&mut ProgressHandle) + Send + 'static) {
        self.progress_handler = Some(Box::new(progress));
    }

    /// Provide a callback to be called whenever a download is retried. This can
    /// be used to customize the retry time, or abort the download.
    pub fn on_retry(&mut self, retry: impl FnMut(&mut RetryHandle) + Send + 'static) {
        self.retry_handler = Box::new(retry);
    }

    /// Set the destination path for the downloaded file.  This can be a file to
    /// store the resulting download in, or a directory in which case the
    /// filename will be determined from the URL or the server's `Content-Disposition`
    /// header.  If this is not set, the current working directory will be used.
    pub fn destination(&mut self, destination: impl AsRef<Path>) {
        self.destination = Some(destination.as_ref().to_owned());
    }

    /// Set the etag for this file.
    pub fn etag(&mut self, etag: impl Into<String>) {
        self.user_provided_local_file_info.etag = Some(etag.into());
    }

    /// Set the last modified time for this file.
    pub fn last_modified(&mut self, last_modified: impl Into<String>) {
        self.user_provided_local_file_info.last_modified = Some(last_modified.into());
    }

    /// Get the destination path or folder.
    pub(crate) fn configured_destination(&self) -> Result<PathBuf, Error> {
        match &self.destination {
            Some(path) => Ok(path.clone()),
            None => std::env::current_dir().map_err(|e| Error::Write {
                action: "determining current directory",
                path: PathBuf::from("."),
                cause: e,
            }),
        }
    }
}

macro_rules! config_proxy {
    ( ) => {
        /// Set the user agent for this download.
        pub fn user_agent<V>(mut self, user_agent: V) -> Self
        where
            http::HeaderValue: TryFrom<V>,
            <http::HeaderValue as TryFrom<V>>::Error: Into<http::Error>,
        {
            self.config.user_agent(user_agent);
            self
        }

        /// Add a header to this download.
        pub fn header<K, V>(mut self, key: K, value: V) -> Self
        where
            K: http::header::IntoHeaderName,
            http::HeaderValue: TryFrom<V>,
            <http::HeaderValue as TryFrom<V>>::Error: Into<http::Error>,
        {
            self.config.header(key, value);
            self
        }

        /// Add a set of Headers to the existing ones on this download.
        /// The headers will be merged in to any already set.
        pub fn headers(mut self, headers: http::HeaderMap) -> Self {
            self.config.headers(headers);
            self
        }

        /// Override the the maxmimum number of times to consecutively retry the
        /// download without making any progress. Pass in `None` to retry forever.
        pub fn max_retries(mut self, max_retries: Option<u64>) -> Self {
            self.config.max_retries(max_retries);
            self
        }

        /// Set the progress reporter for this download.  The given reporter will
        /// be called periodically as data is downloaded.
        pub fn on_progress(
            mut self,
            progress: impl FnMut(&mut $crate::ProgressHandle) + Send + 'static,
        ) -> Self {
            self.config.on_progress(progress);
            self
        }

        /// Set the destination path for the downloaded file.  This can be a file to
        /// store the resulting download in, or a directory in which case the
        /// filename will be determined from the URL or the server's `Content-Disposition`
        /// header.  If this is not set, the current working directory will be used.
        ///
        /// Downloaded files will be saved to a temporary `.part` file in the same
        /// folder, and then renamed to the final destination when the download is complete.
        ///
        pub fn destination(mut self, destination: impl AsRef<std::path::Path>) -> Self {
            self.config.destination(destination);
            self
        }

        /// Set the etag for this file.  If you have already downloaded part of the
        /// file and know the etag, setting this will allow the download to verify
        /// that the file has not changed on the server before resuming.  If neither
        /// this nor the last modified time are set, then the mtime of the existing
        /// file on disk will be used in place of the last modified time.
        pub fn etag(mut self, etag: impl Into<String>) -> Self {
            self.config.etag(etag);
            self
        }

        /// Set the last modified time for this file.  If you have already downloaded
        /// part of the file and know the last modified time, setting this will allow
        /// the download to verify that the file has not changed on the server before
        /// resuming.  If neither this nor the etag are set, then the mtime of the existing
        /// file on disk will be used in place of the last modified time.
        pub fn last_modified(mut self, last_modified: impl Into<String>) -> Self {
            self.config.last_modified(last_modified);
            self
        }
    };
}

pub(crate) use config_proxy;
