use http::{HeaderMap, HeaderName, HeaderValue, header::IntoHeaderName};

use crate::Error;

/// Insert a header into the header map, replacing any existing headers of the same name.
pub fn insert_header<K, V>(headers: &mut HeaderMap, key: K, value: V) -> Result<(), Error>
where
    K: IntoHeaderName,
    HeaderValue: TryFrom<V>,
    <HeaderValue as TryFrom<V>>::Error: Into<http::Error>,
{
    match value.try_into() {
        Ok(v) => {
            headers.insert(key, v);
        }
        Err(e) => {
            return Err(Error::InvalidHeader {
                cause: e.into().to_string(),
            });
        }
    };

    Ok(())
}

/// Append a header to a `HeaderMap`.
pub fn append_header<K, V>(headers: &mut HeaderMap, key: K, value: V) -> Result<(), Error>
where
    K: IntoHeaderName,
    HeaderValue: TryFrom<V>,
    <HeaderValue as TryFrom<V>>::Error: Into<http::Error>,
{
    match value.try_into() {
        Ok(v) => {
            headers.append(key, v);
        }
        Err(e) => {
            return Err(Error::InvalidHeader {
                cause: e.into().to_string(),
            });
        }
    };

    Ok(())
}

/// Append all values from `new_headers` to `headers`.
pub fn append_all_headers<T>(headers: &mut HeaderMap<T>, new_headers: HeaderMap<T>) {
    let mut key = HeaderName::from_static("x-placeholder");
    for (k, value) in new_headers.into_iter() {
        if let Some(k) = k {
            key = k;
        }
        headers.append(&key, value);
    }
}

#[cfg(test)]
mod tests {
    use http::HeaderValue;

    use super::*;

    #[test]
    fn should_insert_headers() {
        let mut headers = HeaderMap::new();
        insert_header(&mut headers, "a", "a").unwrap();
        insert_header(&mut headers, "a", "b").unwrap();
        assert_eq!(headers.get("a").unwrap(), "b");
    }

    #[test]
    fn should_append_headers() {
        let mut headers = HeaderMap::new();
        headers.insert("a", HeaderValue::from_static("a"));

        let mut new_headers = HeaderMap::new();
        new_headers.insert("b", HeaderValue::from_static("b"));

        append_all_headers(&mut headers, new_headers);

        assert_eq!(headers.get("a").unwrap(), "a");
        assert_eq!(headers.get("b").unwrap(), "b");
    }

    #[test]
    fn should_not_remove_existing_headers() {
        let mut headers = HeaderMap::new();
        headers.insert("a", HeaderValue::from_static("a"));

        let mut new_headers = HeaderMap::new();
        new_headers.insert("a", HeaderValue::from_static("b"));

        append_all_headers(&mut headers, new_headers);

        let mut iter = headers.get_all("a").iter();
        assert_eq!(iter.next().unwrap(), "a");
        assert_eq!(iter.next().unwrap(), "b");
        assert_eq!(iter.next(), None);
    }
}
