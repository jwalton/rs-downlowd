use crate::Error;

/// A trait for converting a type into a `url::Url`, heavily inspired by
/// `reqwest::IntoUrl`.
pub trait IntoUrl: IntoUrlSealed {}

pub trait IntoUrlSealed {
    fn into_url(self) -> Result<url::Url, Error>;
}

impl IntoUrl for &str {}
impl IntoUrlSealed for &str {
    fn into_url(self) -> Result<url::Url, Error> {
        url::Url::parse(self).map_err(|e| Error::InvalidUrl {
            cause: e.to_string(),
        })
    }
}

impl IntoUrl for &String {}
impl IntoUrlSealed for &String {
    fn into_url(self) -> Result<url::Url, Error> {
        (&**self).into_url()
    }
}

impl IntoUrl for String {}
impl IntoUrlSealed for String {
    fn into_url(self) -> Result<url::Url, Error> {
        (&*self).into_url()
    }
}

impl IntoUrl for url::Url {}
impl IntoUrlSealed for url::Url {
    fn into_url(self) -> Result<url::Url, Error> {
        Ok(self)
    }
}

impl IntoUrl for http::Uri {}
impl IntoUrlSealed for http::Uri {
    fn into_url(self) -> Result<url::Url, Error> {
        self.to_string().into_url()
    }
}
