use crate::Error;

/// A trait for converting a type into a `url::Url`, heavily inspired by
/// `reqwest::IntoUri`.
pub trait IntoUri: IntoUriSealed {}

pub trait IntoUriSealed {
    fn into_uri(self) -> Result<http::Uri, Error>;
}

impl IntoUri for &str {}
impl IntoUriSealed for &str {
    fn into_uri(self) -> Result<http::Uri, Error> {
        self.parse::<http::Uri>().map_err(|e| Error::InvalidUrl {
            cause: e.to_string(),
        })
    }
}

impl IntoUri for &String {}
impl IntoUriSealed for &String {
    fn into_uri(self) -> Result<http::Uri, Error> {
        (&**self).into_uri()
    }
}

impl IntoUri for String {}
impl IntoUriSealed for String {
    fn into_uri(self) -> Result<http::Uri, Error> {
        (&*self).into_uri()
    }
}

#[cfg(feature="async")]
impl IntoUri for url::Url {}
#[cfg(feature="async")]
impl IntoUriSealed for url::Url {
    fn into_uri(self) -> Result<http::Uri, Error> {
        self.to_string().into_uri()
    }
}

impl IntoUri for http::Uri {}
impl IntoUriSealed for http::Uri {
    fn into_uri(self) -> Result<http::Uri, Error> {
        self.to_string().into_uri()
    }
}
