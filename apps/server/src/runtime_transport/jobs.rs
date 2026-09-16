//! Bounded native job exchanges over the same deployment-authorized TLS client.
//! Missing/timeout are observations, never proof that a job cannot execute later.
use super::{json, RuntimeTransport};
use chrono::{DateTime, Utc};
use contracts::{runtime::RuntimeProbeFailure, runtime_jobs::*, Id};
use domain::runtime_jobs as boundary;
use reqwest::{header, Response, StatusCode};
use serde::de::DeserializeOwned;
use std::fmt;
use url::Url;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RuntimeRequestError {
    Authentication,
    Unavailable,
    Missing,
    Conflict,
    Closed,
    Contract,
    ResponseLimit,
}
impl fmt::Display for RuntimeRequestError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Authentication => "runtime authentication failed",
            Self::Unavailable => "runtime exchange is unavailable or its outcome is unknown",
            Self::Missing => "runtime resource was not found",
            Self::Conflict => "runtime identity conflicts with its immutable receipt",
            Self::Closed => "runtime identity has been permanently closed",
            Self::Contract => "runtime exchange violated its contract",
            Self::ResponseLimit => "runtime response exceeded its bound",
        })
    }
}
impl std::error::Error for RuntimeRequestError {}
impl RuntimeRequestError {
    pub(super) fn probe(self) -> RuntimeProbeFailure {
        match self {
            Self::Authentication => RuntimeProbeFailure::Authentication,
            Self::Unavailable => RuntimeProbeFailure::Unavailable,
            Self::ResponseLimit => RuntimeProbeFailure::ResponseLimit,
            _ => RuntimeProbeFailure::ContractUnsupported,
        }
    }
}

/// Exact validated manifest bytes, retained for immutable provenance rather than re-encoding.
/// Intentionally no Debug implementation for arbitrary remote payloads.
pub struct ReceivedRuntimeResult {
    pub manifest: ResultManifestV1,
    pub raw_document: Vec<u8>,
}

fn response_status(
    response: &Response,
    accepted: &[StatusCode],
) -> Result<(), RuntimeRequestError> {
    if accepted.contains(&response.status()) {
        return Ok(());
    }
    Err(match response.status().as_u16() {
        401 | 403 => RuntimeRequestError::Authentication,
        404 => RuntimeRequestError::Missing,
        409 => RuntimeRequestError::Conflict,
        410 => RuntimeRequestError::Closed,
        413 => RuntimeRequestError::ResponseLimit,
        408 | 425 | 429 | 500..=599 => RuntimeRequestError::Unavailable,
        _ => RuntimeRequestError::Contract,
    })
}

fn media(response: &Response, expected: &str) -> Result<(), RuntimeRequestError> {
    let mut values = response.headers().get_all(header::CONTENT_TYPE).iter();
    let value = values
        .next()
        .and_then(|value| value.to_str().ok())
        .ok_or(RuntimeRequestError::Contract)?;
    if values.next().is_some() {
        return Err(RuntimeRequestError::Contract);
    }
    let mut parts = value.split(';');
    if !parts
        .next()
        .is_some_and(|value| value.trim().eq_ignore_ascii_case(expected))
    {
        return Err(RuntimeRequestError::Contract);
    }
    // Only the explicit UTF-8 JSON parameter is meaningful for these strict wire bodies.
    for parameter in parts {
        if expected != "application/json" || !parameter.trim().eq_ignore_ascii_case("charset=utf-8")
        {
            return Err(RuntimeRequestError::Contract);
        }
    }
    let mut encodings = response.headers().get_all(header::CONTENT_ENCODING).iter();
    if encodings.next().is_some_and(|value| value != "identity") || encodings.next().is_some() {
        return Err(RuntimeRequestError::Contract);
    }
    Ok(())
}

async fn body(mut response: Response, maximum: usize) -> Result<Vec<u8>, RuntimeRequestError> {
    if maximum == 0
        || response
            .content_length()
            .is_some_and(|size| size > maximum as u64)
    {
        return Err(RuntimeRequestError::ResponseLimit);
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| RuntimeRequestError::Unavailable)?
    {
        if chunk.len() > maximum.saturating_sub(bytes.len()) {
            return Err(RuntimeRequestError::ResponseLimit);
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

impl RuntimeTransport {
    pub(super) fn resource(&self, segments: &[&str]) -> Result<Url, RuntimeRequestError> {
        let mut url = self.origin.clone();
        url.path_segments_mut()
            .map_err(|_| RuntimeRequestError::Contract)?
            .clear()
            .extend(["runtime", "v1"])
            .extend(segments.iter().copied());
        Ok(url)
    }

    pub(super) async fn json_response<T: DeserializeOwned>(
        &self,
        response: Response,
        accepted: &[StatusCode],
        maximum: usize,
    ) -> Result<(T, Vec<u8>), RuntimeRequestError> {
        response_status(&response, accepted)?;
        media(&response, "application/json")?;
        let bytes = body(response, maximum).await?;
        json::verify(&bytes, &self.credential).map_err(|_| RuntimeRequestError::Contract)?;
        let parsed = serde_json::from_slice(&bytes).map_err(|_| RuntimeRequestError::Contract)?;
        Ok((parsed, bytes))
    }

    async fn received_status(
        &self,
        response: Response,
        external_id: &str,
        accepted: &[StatusCode],
    ) -> Result<RuntimeJobStatusV1, RuntimeRequestError> {
        let (run, attempt) =
            boundary::parse_external_id(external_id).map_err(|_| RuntimeRequestError::Contract)?;
        let (status, _) = self
            .json_response(response, accepted, boundary::MAX_RESULT_MANIFEST_BYTES)
            .await?;
        boundary::status(&status, run, attempt, Utc::now())
            .map_err(|_| RuntimeRequestError::Contract)?;
        Ok(status)
    }

    /// The caller must commit its send intent before invoking this once for an identity.
    pub async fn submit_job(
        &self,
        spec: &JobSpecV1,
    ) -> Result<RuntimeJobStatusV1, RuntimeRequestError> {
        boundary::spec_shape(spec).map_err(|_| RuntimeRequestError::Contract)?;
        let bytes = serde_json::to_vec(spec).map_err(|_| RuntimeRequestError::Contract)?;
        if bytes.len() > boundary::MAX_JOB_REQUEST_BYTES {
            return Err(RuntimeRequestError::ResponseLimit);
        }
        let response = self
            .client
            .post(self.resource(&["jobs"])?)
            .header(header::CONTENT_TYPE, "application/json")
            .body(bytes)
            .send()
            .await
            .map_err(|_| RuntimeRequestError::Unavailable)?;
        self.received_status(
            response,
            &spec.external_job_id,
            &[StatusCode::OK, StatusCode::CREATED, StatusCode::ACCEPTED],
        )
        .await
    }

    /// A Missing result is deliberately not transformed into a terminal cancellation.
    pub async fn job_status(
        &self,
        external_id: &str,
    ) -> Result<RuntimeJobStatusV1, RuntimeRequestError> {
        boundary::parse_external_id(external_id).map_err(|_| RuntimeRequestError::Contract)?;
        let response = self
            .client
            .get(self.resource(&["jobs", external_id])?)
            .send()
            .await
            .map_err(|_| RuntimeRequestError::Unavailable)?;
        self.received_status(response, external_id, &[StatusCode::OK])
            .await
    }

    /// Cancellation receipt may still be CANCEL_REQUESTED; only its durable terminal
    /// state can establish that a late original submission will never be started.
    pub async fn cancel_job(
        &self,
        external_id: &str,
        command: &RuntimeCancelV1,
    ) -> Result<RuntimeJobStatusV1, RuntimeRequestError> {
        let identity =
            boundary::parse_external_id(external_id).map_err(|_| RuntimeRequestError::Contract)?;
        if identity != (command.run_id, command.attempt_no) {
            return Err(RuntimeRequestError::Contract);
        }
        let response = self
            .client
            .post(self.resource(&["jobs", external_id, "cancel"])?)
            .json(command)
            .send()
            .await
            .map_err(|_| RuntimeRequestError::Unavailable)?;
        self.received_status(
            response,
            external_id,
            &[StatusCode::OK, StatusCode::ACCEPTED],
        )
        .await
    }

    pub async fn job_result(
        &self,
        spec: &JobSpecV1,
        submitted_not_before: DateTime<Utc>,
    ) -> Result<ReceivedRuntimeResult, RuntimeRequestError> {
        boundary::spec_shape(spec).map_err(|_| RuntimeRequestError::Contract)?;
        let response = self
            .client
            .get(self.resource(&["jobs", &spec.external_job_id, "result"])?)
            .send()
            .await
            .map_err(|_| RuntimeRequestError::Unavailable)?;
        let (manifest, raw_document) = self
            .json_response(
                response,
                &[StatusCode::OK],
                boundary::MAX_RESULT_MANIFEST_BYTES,
            )
            .await?;
        boundary::manifest(&manifest, spec, submitted_not_before, Utc::now())
            .map_err(|_| RuntimeRequestError::Contract)?;
        Ok(ReceivedRuntimeResult {
            manifest,
            raw_document,
        })
    }

    pub async fn upload_object(
        &self,
        id: Id,
        version: &str,
        bytes: Vec<u8>,
    ) -> Result<RuntimeObjectReceiptV1, RuntimeRequestError> {
        boundary::storage_version(version).map_err(|_| RuntimeRequestError::Contract)?;
        if bytes.is_empty() || bytes.len() as u64 > boundary::MAX_INPUT_OBJECT_BYTES {
            return Err(RuntimeRequestError::ResponseLimit);
        }
        let expected = bytes.len() as u64;
        let version_header =
            header::HeaderValue::from_str(version).map_err(|_| RuntimeRequestError::Contract)?;
        let response = self
            .client
            .put(self.resource(&["objects", &id.to_string()])?)
            .header(header::CONTENT_TYPE, "application/octet-stream")
            .header("x-qz-storage-version", version_header)
            .body(bytes)
            .send()
            .await
            .map_err(|_| RuntimeRequestError::Unavailable)?;
        let (receipt, _): (RuntimeObjectReceiptV1, _) = self
            .json_response(response, &[StatusCode::OK, StatusCode::CREATED], 4096)
            .await?;
        if receipt.artifact_id != id
            || receipt.storage_version != version
            || receipt.byte_count.get() != expected
        {
            return Err(RuntimeRequestError::Contract);
        }
        Ok(receipt)
    }

    /// Downloads are scoped to the exact manifest's native identity, never a remote URL.
    /// Arrow/JSON/Wasm scientific validation remains mandatory after this byte boundary.
    pub async fn job_artifact(
        &self,
        external_id: &str,
        output: &RuntimeOutputV1,
    ) -> Result<Vec<u8>, RuntimeRequestError> {
        boundary::parse_external_id(external_id).map_err(|_| RuntimeRequestError::Contract)?;
        if output.storage_version.get() != 1
            || output.byte_count.get() == 0
            || output.byte_count.get() > boundary::MAX_INPUT_OBJECT_BYTES
        {
            return Err(RuntimeRequestError::Contract);
        }
        let response = self
            .client
            .get(self.resource(&[
                "jobs",
                external_id,
                "artifacts",
                &output.storage_ref.to_string(),
            ])?)
            .header(header::ACCEPT, &output.media_type)
            .send()
            .await
            .map_err(|_| RuntimeRequestError::Unavailable)?;
        response_status(&response, &[StatusCode::OK])?;
        media(&response, &output.media_type)?;
        let maximum = usize::try_from(output.byte_count.get())
            .map_err(|_| RuntimeRequestError::ResponseLimit)?;
        let bytes = body(response, maximum).await?;
        if bytes.len() != maximum
            || bytes
                .windows(self.credential.len())
                .any(|part| part == self.credential.as_bytes())
        {
            return Err(RuntimeRequestError::Contract);
        }
        if output.media_type == "application/json" {
            json::verify(&bytes, &self.credential).map_err(|_| RuntimeRequestError::Contract)?;
        }
        Ok(bytes)
    }
}
