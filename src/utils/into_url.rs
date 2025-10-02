use crate::Error;

/// A trait for converting a type into a `url::Url`, heavily inspired by
/// `reqwest::IntoUrl`.
pub trait IntoUrl: IntoUrlSealed {}

impl IntoUrl for &str {}
impl IntoUrl for &String {}
impl IntoUrl for String {}
impl IntoUrl for url::Url {}
impl IntoUrl for &url::Url {}
impl IntoUrl for http::Uri {}
impl IntoUrl for &http::Uri {}

pub trait IntoUrlSealed {
    fn into_url(self) -> Result<url::Url, Error>;
}

impl IntoUrlSealed for &str {
    fn into_url(self) -> Result<url::Url, Error> {
        url::Url::parse(self).map_err(|e| Error::InvalidUrl {
            cause: e.to_string(),
        })
    }
}

impl IntoUrlSealed for &String {
    fn into_url(self) -> Result<url::Url, Error> {
        (&**self).into_url()
    }
}

impl IntoUrlSealed for String {
    fn into_url(self) -> Result<url::Url, Error> {
        (&*self).into_url()
    }
}

impl IntoUrlSealed for url::Url {
    fn into_url(self) -> Result<url::Url, Error> {
        Ok(self)
    }
}

impl IntoUrlSealed for &url::Url {
    fn into_url(self) -> Result<url::Url, Error> {
        Ok(self.clone())
    }
}

impl IntoUrlSealed for http::Uri {
    fn into_url(self) -> Result<url::Url, Error> {
        (&self).into_url()
    }
}

impl IntoUrlSealed for &http::Uri {
    fn into_url(self) -> Result<url::Url, Error> {
        let s = self.to_string();
        s.into_url()
    }
}
