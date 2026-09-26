//! Shared native service transport. Authority and business routes stay in each adapter.
use contracts::http::Problem;
use reqwest::{header, Client, ClientBuilder, RequestBuilder, Response, Url};
use serde::de::DeserializeOwned;
use std::time::Duration;

pub(crate) const MAX_JSON_BYTES: usize = 1024 * 1024;

// Never retain native error chains: URLs, credentials and response values may be private.
pub(crate) enum Failure {
    Configuration,
    Unavailable,
    Contract,
    ResponseLimit,
    Rejected(Box<Problem>),
}
type Result<T> = std::result::Result<T, Failure>;

pub(crate) fn origin(value: &str, development_http: bool) -> Result<Url> {
    if value.trim() != value {
        return Err(Failure::Configuration);
    }
    crate::WebPolicy::new(value, ([127, 0, 0, 1], 0).into(), development_http)
        .map_err(|_| Failure::Configuration)?;
    Url::parse(value).map_err(|_| Failure::Configuration)
}

pub(crate) fn bearer(token: &str) -> Result<header::HeaderMap> {
    let mut value = header::HeaderValue::from_str(&format!("Bearer {token}"))
        .map_err(|_| Failure::Configuration)?;
    value.set_sensitive(true);
    let mut headers = header::HeaderMap::new();
    headers.insert(header::AUTHORIZATION, value);
    Ok(headers)
}

pub(crate) fn builder(mut headers: header::HeaderMap, timeout: Duration) -> ClientBuilder {
    headers.insert(
        header::ACCEPT,
        header::HeaderValue::from_static("application/json"),
    );
    headers.insert(
        header::ACCEPT_ENCODING,
        header::HeaderValue::from_static("identity"),
    );
    Client::builder()
        .default_headers(headers)
        .redirect(reqwest::redirect::Policy::none())
        .retry(reqwest::retry::never())
        .no_proxy()
        .no_gzip()
        .no_brotli()
        .no_deflate()
        .no_zstd()
        .connect_timeout(Duration::from_secs(3))
        .timeout(timeout)
}

pub(crate) async fn send(request: RequestBuilder) -> Result<Response> {
    request.send().await.map_err(|_| Failure::Unavailable)
}

pub(crate) fn media(response: &Response, expected: &str) -> Result<()> {
    if response
        .headers()
        .get_all(header::CONTENT_TYPE)
        .iter()
        .count()
        != 1
    {
        return Err(Failure::Contract);
    }
    let mut encodings = response.headers().get_all(header::CONTENT_ENCODING).iter();
    if encodings.next().is_some_and(|value| value != "identity") || encodings.next().is_some() {
        return Err(Failure::Contract);
    }
    let value = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .ok_or(Failure::Contract)?;
    let mut parts = value.split(';');
    if !parts
        .next()
        .is_some_and(|part| part.trim().eq_ignore_ascii_case(expected))
        || parts.any(|part| !part.trim().eq_ignore_ascii_case("charset=utf-8"))
    {
        return Err(Failure::Contract);
    }
    Ok(())
}

pub(crate) async fn body(mut response: Response, maximum: usize) -> Result<Vec<u8>> {
    if response
        .content_length()
        .is_some_and(|size| size > maximum as u64)
    {
        return Err(Failure::ResponseLimit);
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| Failure::Unavailable)? {
        if chunk.len() > maximum.saturating_sub(bytes.len()) {
            return Err(Failure::ResponseLimit);
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

pub(crate) fn verify(bytes: &[u8], credential: &str) -> Result<()> {
    crate::runtime_transport::verify_native_json(bytes, credential).map_err(|_| Failure::Contract)
}

pub(crate) fn decode<T: DeserializeOwned>(bytes: &[u8], credential: &str) -> Result<T> {
    verify(bytes, credential)?;
    serde_json::from_slice(bytes).map_err(|_| Failure::Contract)
}

pub(crate) async fn checked(
    response: Response,
    expected: u16,
    credential: &str,
) -> Result<Response> {
    let status = response.status().as_u16();
    if status == expected {
        return Ok(response);
    }
    if !(400..=599).contains(&status) {
        return Err(Failure::Contract);
    }
    media(&response, "application/problem+json")?;
    let problem: Problem = decode(&body(response, MAX_JSON_BYTES).await?, credential)?;
    if problem.status != status || !problem.kind.starts_with("urn:quazonai:problem:") {
        return Err(Failure::Contract);
    }
    Err(Failure::Rejected(Box::new(problem)))
}
