use chrono::{DateTime, Utc};

pub async fn head_url(url: &str) -> (Option<DateTime<Utc>>, Option<String>) {
    let http_client = reqwest::Client::new();
    let response = http_client.head(url).send().await.expect("url to exist");

    assert_eq!(response.status(), 200, "Resource {url} should exist");

    let last_modified = response
        .headers()
        .get(reqwest::header::LAST_MODIFIED)
        .map(|s| {
            chrono::DateTime::parse_from_rfc2822(s.to_str().expect("valid string"))
                .expect("valid date")
                .with_timezone(&chrono::Utc)
        });

    let etag = response
        .headers()
        .get(reqwest::header::ETAG)
        .map(|s| s.to_str().expect("valid string").to_owned());

    (last_modified, etag)
}
