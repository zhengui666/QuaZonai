//! Existing scoped downstream credential transport. No native handle crosses an
//! await. Unknown responses remain caller-visible and retry the unchanged DTO/key.
use crate::paper_service::initial_execution::service_http;
use anyhow::{Result, anyhow, ensure};
use contracts::{
    Id,
    capital_exit::*,
    control::{CommandResult, Page},
};
use reqwest::{
    Client, Url,
    header::{AUTHORIZATION, HeaderMap, HeaderValue},
};
use serde::{Serialize, de::DeserializeOwned};
use std::time::Duration;

#[derive(Clone)]
pub struct CapitalExitOwnerTransport {
    origin: Url,
    client: Client,
    credential: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use contracts::account_observation::*;

    #[tokio::test]
    async fn registered_observation_requires_exact_single_source_header() {
        use axum::{
            Json, Router,
            http::{HeaderMap, HeaderValue, StatusCode},
            routing::post,
        };
        let observation: AccountObservationSubmitV1 = serde_json::from_str(include_str!(
            "../../../tests/contracts/native-account-paper-snapshot.json"
        ))
        .unwrap();
        let request = AccountObservationSubmitV2 {
            schema_version: NativeClientObservationSchemaV2,
            native_client_id: "SYNTHETIC-OBSERVATION-CLIENT".into(),
            observation,
        };
        let source = Id::new();
        let receipt = AccountObservationReceiptV2 {
            schema_version: NativeClientObservationSchemaV2,
            replayed: false,
            native_client_id: request.native_client_id.clone(),
            resource: AccountObservationV1 {
                id: Id::new(),
                source_id: source,
                downstream_id: Id::new(),
                observation: request.observation.clone(),
                gap_before: false,
                received_at: chrono::Utc::now(),
            },
        };
        for mode in ["missing", "wrong", "duplicate", "exact"] {
            let mut headers = HeaderMap::new();
            if mode != "missing" {
                headers.insert(
                    "x-qz-capital-exit-source",
                    HeaderValue::from_str(
                        &if mode == "wrong" { Id::new() } else { source }.to_string(),
                    )
                    .unwrap(),
                );
            }
            if mode == "duplicate" {
                headers.append(
                    "x-qz-capital-exit-source",
                    HeaderValue::from_str(&source.to_string()).unwrap(),
                );
            }
            let body = receipt.clone();
            let router = Router::new().route(
                "/api/v2/forward/client-account-observations",
                post(move || {
                    let headers = headers.clone();
                    let body = body.clone();
                    async move { (StatusCode::CREATED, headers, Json(body)) }
                }),
            );
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let origin = format!("http://{}/", listener.local_addr().unwrap());
            let task = tokio::spawn(async move {
                axum::serve(listener, router).await.unwrap();
            });
            let transport = CapitalExitOwnerTransport::new(
                &origin,
                b"synthetic-protocol-only-no-real-authority",
            )
            .unwrap();
            let result = transport.submit_observation(&request).await;
            assert_eq!(result.is_ok(), mode == "exact", "{mode}");
            task.abort();
            let _ = task.await;
        }
    }
}
impl CapitalExitOwnerTransport {
    pub(crate) fn from_existing(client: Client, origin: Url, credential: String) -> Self {
        Self {
            client,
            origin,
            credential,
        }
    }
    /// Reuses an existing configured downstream credential; does not create or
    /// persist credentials, and will never follow a redirect with authorization.
    pub fn new(origin: &str, credential: &[u8]) -> Result<Self> {
        let origin = Url::parse(origin)?;
        ensure!(
            origin.username().is_empty()
                && origin.password().is_none()
                && origin.query().is_none()
                && origin.fragment().is_none()
                && origin.path() == "/",
            "capital_exit_transport_origin"
        );
        let loopback = origin
            .host_str()
            .and_then(|h| h.parse::<std::net::IpAddr>().ok())
            .is_some_and(|ip| ip.is_loopback());
        ensure!(
            origin.scheme() == "https" || (origin.scheme() == "http" && loopback),
            "capital_exit_transport_https_required"
        );
        ensure!(
            !credential.is_empty() && credential.iter().all(|b| b.is_ascii_graphic()),
            "capital_exit_transport_credential"
        );
        let mut bytes = b"Bearer ".to_vec();
        bytes.extend_from_slice(credential);
        let mut authorization = HeaderValue::from_bytes(&bytes)
            .map_err(|_| anyhow!("capital_exit_transport_credential"))?;
        authorization.set_sensitive(true);
        let mut headers = HeaderMap::new();
        headers.insert(AUTHORIZATION, authorization);
        let mut client = Client::builder()
            .default_headers(headers)
            .redirect(reqwest::redirect::Policy::none())
            .retry(reqwest::retry::never())
            .timeout(Duration::from_secs(20));
        if loopback {
            client = client.no_proxy();
        }
        Ok(Self {
            origin,
            client: client.build()?,
            credential: String::from_utf8(credential.to_vec())
                .map_err(|_| anyhow!("capital_exit_transport_credential"))?,
        })
    }
    fn url(&self, path: &str) -> Result<Url> {
        Ok(self.origin.join(path)?)
    }
    async fn get<T: DeserializeOwned>(&self, path: &str, cursor: Option<Id>) -> Result<T> {
        let mut url = self.url(path)?;
        if let Some(cursor) = cursor {
            url.query_pairs_mut()
                .append_pair("cursor", &cursor.to_string());
        }
        let response = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|_| anyhow!("capital_exit_transport_read_unavailable"))?;
        ensure!(
            response.status().is_success(),
            "capital_exit_transport_read_status:{}",
            response.status().as_u16()
        );
        service_http::media(&response, "application/json")
            .map_err(|_| anyhow!("capital_exit_transport_invalid_response"))?;
        let bytes = service_http::body(response, None)
            .await
            .map_err(|_| anyhow!("capital_exit_transport_invalid_response"))?;
        service_http::decode(&bytes, &self.credential)
            .map_err(|_| anyhow!("capital_exit_transport_invalid_response"))
    }
    async fn post<T: DeserializeOwned>(
        &self,
        path: &str,
        key: &str,
        body: &impl Serialize,
    ) -> Result<T> {
        let response = self
            .client
            .post(self.url(path)?)
            .header("Idempotency-Key", key)
            .json(body)
            .send()
            .await
            .map_err(|_| {
                anyhow!("capital_exit_transport_outcome_unknown_reconcile_original_identity")
            })?;
        ensure!(
            response.status().is_success(),
            "capital_exit_transport_write_status:{}",
            response.status().as_u16()
        );
        service_http::media(&response, "application/json").map_err(|_| {
            anyhow!("capital_exit_transport_outcome_unknown_reconcile_original_identity")
        })?;
        let bytes = service_http::body(response, None)
            .await
            .map_err(|_| {
                anyhow!("capital_exit_transport_outcome_unknown_reconcile_original_identity")
            })?;
        service_http::decode(&bytes, &self.credential).map_err(|_| {
            anyhow!("capital_exit_transport_outcome_unknown_reconcile_original_identity")
        })
    }
    pub(crate) async fn submit_observation(
        &self,
        request: &contracts::account_observation::AccountObservationSubmitV2,
    ) -> Result<contracts::account_observation::AccountObservationReceiptV2> {
        // The source/session/sequence supplies the existing intake replay
        // identity. A normal successful intake is not owner-registration proof.
        let response = self
            .client
            .post(self.url("/api/v2/forward/client-account-observations")?)
            .json(request)
            .send()
            .await
            .map_err(|_| {
                anyhow!("capital_exit_transport_outcome_unknown_reconcile_original_identity")
            })?;
        ensure!(
            response.status().as_u16() == 201,
            "capital_exit_transport_write_status:{}",
            response.status().as_u16()
        );
        let mut bindings = response
            .headers()
            .get_all("x-qz-capital-exit-source")
            .iter();
        let registered: Id = bindings
            .next()
            .and_then(|value| value.to_str().ok())
            .and_then(|value| Id::try_from(value.to_owned()).ok())
            .ok_or_else(|| anyhow!("capital_exit_owner_registration_unavailable"))?;
        ensure!(
            bindings.next().is_none(),
            "capital_exit_owner_registration_unavailable"
        );
        service_http::media(&response, "application/json").map_err(|_| {
            anyhow!("capital_exit_transport_outcome_unknown_reconcile_original_identity")
        })?;
        let bytes = service_http::body(response, None)
            .await
            .map_err(|_| {
                anyhow!("capital_exit_transport_outcome_unknown_reconcile_original_identity")
            })?;
        let receipt: contracts::account_observation::AccountObservationReceiptV2 =
            service_http::decode(&bytes, &self.credential).map_err(|_| {
                anyhow!("capital_exit_transport_outcome_unknown_reconcile_original_identity")
            })?;
        ensure!(
            registered == receipt.resource.source_id,
            "capital_exit_owner_registration_source_mismatch"
        );
        Ok(receipt)
    }
    pub async fn pending_assessments(
        &self,
        cursor: Option<Id>,
    ) -> Result<Page<CapitalExitPreviewV1>> {
        self.get("/api/v2/downstream/capital-exit-assessments", cursor)
            .await
    }
    pub async fn pending_intents(&self, cursor: Option<Id>) -> Result<Page<CapitalExitViewV1>> {
        self.get("/api/v2/downstream/capital-exits", cursor).await
    }
    pub async fn submit_assessment(
        &self,
        request: &CapitalExitOwnerAssessmentV1,
    ) -> Result<CommandResult<CapitalExitAssessmentReceiptV1>> {
        self.post(
            "/api/v2/downstream/capital-exit-assessments",
            &request.external_message_id,
            request,
        )
        .await
    }
    pub async fn claim(
        &self,
        intent: Id,
        key: &str,
        request: &CapitalExitClaimV1,
    ) -> Result<CommandResult<CapitalExitViewV1>> {
        self.post(
            &format!("/api/v2/capital-exits/{intent}/claim"),
            key,
            request,
        )
        .await
    }
    pub async fn submit_evidence(
        &self,
        request: &CapitalExitOwnerEvidenceV1,
    ) -> Result<CommandResult<CapitalExitViewV1>> {
        self.post(
            &format!("/api/v2/capital-exits/{}/evidence", request.intent_id),
            &request.external_message_id,
            request,
        )
        .await
    }
}
